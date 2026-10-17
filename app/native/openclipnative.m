#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreImage/CoreImage.h>
#import <CoreText/CoreText.h>
#import <ImageIO/ImageIO.h>
#import <Vision/Vision.h>
#import <CoreVideo/CoreVideo.h>
#include <math.h>
#include "openclipnative.h"

#pragma clang diagnostic ignored "-Wdeprecated-declarations"

static char *dup_str(NSString *s) { return strdup(s.UTF8String ?: ""); }
static char *dup_json(id obj) {
    NSData *d = [NSJSONSerialization dataWithJSONObject:obj options:0 error:nil];
    NSString *s = [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding];
    return dup_str(s);
}
static NSString *err_json(NSString *m) { return [NSString stringWithFormat:@"{\"error\":%@}", [[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:@[m] options:0 error:nil] encoding:NSUTF8StringEncoding]]; }

void cn_free(void *p) { free(p); }

// AppKit keeps re-inserting AutoFill into any menu titled "Edit"; remove it whenever a menu opens.
static void strip_autofill(void) {
    NSMenu *main = NSApp.mainMenu;
    for (NSMenuItem *top in main.itemArray) {
        if (![top.title isEqualToString:@"Edit"]) continue;
        NSMenu *m = top.submenu;
        for (NSInteger i = (NSInteger)m.numberOfItems - 1; i >= 0; i--) {
            NSMenuItem *it = [m itemAtIndex:i];
            NSString *t = it.title;
            if ([t isEqualToString:@"AutoFill"] || [t hasPrefix:@"Start Dictation"] || [t hasPrefix:@"Emoji"]) [m removeItemAtIndex:i];
        }
        // drop a trailing separator left behind
        while (m.numberOfItems > 0 && [m itemAtIndex:m.numberOfItems - 1].isSeparatorItem) [m removeItemAtIndex:m.numberOfItems - 1];
    }
}

void cn_trim_edit_menu(void) {
    [[NSNotificationCenter defaultCenter] addObserverForName:NSMenuDidBeginTrackingNotification object:nil queue:nil usingBlock:^(NSNotification *n) { strip_autofill(); }];
    [[NSNotificationCenter defaultCenter] addObserverForName:NSApplicationDidBecomeActiveNotification object:nil queue:nil usingBlock:^(NSNotification *n) { strip_autofill(); }];
    dispatch_async(dispatch_get_main_queue(), ^{ strip_autofill(); });
    NSUserDefaults *d = [NSUserDefaults standardUserDefaults];
    [d setBool:YES forKey:@"NSDisabledDictationMenuItem"];
    [d setBool:YES forKey:@"NSDisabledCharacterPaletteMenuItem"];
    [d setBool:NO forKey:@"NSAutoFillHeuristicControllerEnabled"];
}

static AVURLAsset *open_asset(const char *path) {
    NSURL *u = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
    return [AVURLAsset URLAssetWithURL:u options:@{AVURLAssetPreferPreciseDurationAndTimingKey: @YES}];
}

// 0 = none, 90 / 180 / 270 = how the stored frames must be rotated to be upright
static int rotation_of(CGAffineTransform t) {
    if (fabs(t.a) < 0.01 && fabs(fabs(t.b) - 1) < 0.01) return t.b > 0 ? 90 : 270;
    if (t.a < -0.99 && t.d < -0.99) return 180;
    return 0;
}

char *cn_probe(const char *path) {
    @autoreleasepool {
        AVURLAsset *a = open_asset(path);
        AVAssetTrack *v = [[a tracksWithMediaType:AVMediaTypeVideo] firstObject];
        if (!v) return dup_str(err_json(@"no video track in this file"));
        CGSize sz = v.naturalSize;
        int rot = rotation_of(v.preferredTransform);
        if (rot == 90 || rot == 270) sz = CGSizeMake(sz.height, sz.width);
        double dur = CMTimeGetSeconds(a.duration);
        double fps = v.nominalFrameRate;
        BOOL audio = [a tracksWithMediaType:AVMediaTypeAudio].count > 0;
        return dup_json(@{@"durationMs": @((long long)llround(dur * 1000)), @"width": @((int)sz.width), @"height": @((int)sz.height),
                          @"fps": @(fps), @"hasAudio": @(audio)});
    }
}

float *cn_decode_audio(const char *path, size_t *n_samples, cn_progress cb, void *ud, char **err) {
    @autoreleasepool {
        *n_samples = 0;
        AVURLAsset *a = open_asset(path);
        AVAssetTrack *t = [[a tracksWithMediaType:AVMediaTypeAudio] firstObject];
        if (!t) { *err = dup_str(@"this video has no audio track"); return NULL; }
        NSError *e = nil;
        AVAssetReader *r = [[AVAssetReader alloc] initWithAsset:a error:&e];
        if (!r) { *err = dup_str(e.localizedDescription ?: @"cannot read file"); return NULL; }
        NSDictionary *set = @{AVFormatIDKey: @(kAudioFormatLinearPCM), AVSampleRateKey: @16000, AVNumberOfChannelsKey: @1,
                              AVLinearPCMBitDepthKey: @32, AVLinearPCMIsFloatKey: @YES, AVLinearPCMIsBigEndianKey: @NO,
                              AVLinearPCMIsNonInterleaved: @NO};
        AVAssetReaderTrackOutput *o = [AVAssetReaderTrackOutput assetReaderTrackOutputWithTrack:t outputSettings:set];
        o.alwaysCopiesSampleData = NO;
        [r addOutput:o];
        if (![r startReading]) { *err = dup_str(r.error.localizedDescription ?: @"cannot decode audio"); return NULL; }
        double dur = CMTimeGetSeconds(a.duration);
        size_t cap = (size_t)(MAX(dur, 1.0) * 16000.0) + 64000, len = 0;
        float *buf = malloc(cap * sizeof(float));
        CMSampleBufferRef sb;
        int tick = 0;
        while ((sb = [o copyNextSampleBuffer])) {
            CMBlockBufferRef bb = CMSampleBufferGetDataBuffer(sb);
            size_t bytes = CMBlockBufferGetDataLength(bb);
            size_t n = bytes / sizeof(float);
            if (len + n > cap) { cap = (len + n) * 3 / 2; buf = realloc(buf, cap * sizeof(float)); }
            CMBlockBufferCopyDataBytes(bb, 0, bytes, buf + len);
            len += n;
            if (++tick % 64 == 0 && cb && dur > 0) {
                if (cb(MIN(100.0, len / 16000.0 / dur * 100.0), ud)) { CFRelease(sb); [r cancelReading]; free(buf); *err = dup_str(@"cancelled"); return NULL; }
            }
            CFRelease(sb);
        }
        if (r.status == AVAssetReaderStatusFailed) { free(buf); *err = dup_str(r.error.localizedDescription ?: @"audio decode failed"); return NULL; }
        *n_samples = len;
        return buf;
    }
}

// ---------------------------------------------------------------- faces

static CGImagePropertyOrientation orientation_for(int rot) {
    switch (rot) {
        case 90: return kCGImagePropertyOrientationRight;
        case 270: return kCGImagePropertyOrientationLeft;
        case 180: return kCGImagePropertyOrientationDown;
        default: return kCGImagePropertyOrientationUp;
    }
}

char *cn_scan_faces(const char *path, int step_ms, cn_progress cb, void *ud, char **err) {
    @autoreleasepool {
        AVURLAsset *a = open_asset(path);
        AVAssetTrack *v = [[a tracksWithMediaType:AVMediaTypeVideo] firstObject];
        if (!v) { *err = dup_str(@"no video track"); return NULL; }
        CGSize nat = v.naturalSize;
        int rot = rotation_of(v.preferredTransform);
        CGSize up = (rot == 90 || rot == 270) ? CGSizeMake(nat.height, nat.width) : nat;
        double dur_ms = CMTimeGetSeconds(a.duration) * 1000.0;
        int nchunks = (int)MAX(1, MIN(6, [[NSProcessInfo processInfo] activeProcessorCount] / 2));
        if (dur_ms < 60000) nchunks = 1;
        long long step = MAX(step_ms, 100);
        long long total_steps = (long long)(dur_ms / step) + 1;
        long long per = (total_steps + nchunks - 1) / nchunks;
        int scan_w = 800;
        double sc = MIN(1.0, scan_w / MAX(nat.width, 1.0));
        int tw = (int)(nat.width * sc) & ~1, th = (int)(nat.height * sc) & ~1;

        NSMutableArray *results = [NSMutableArray arrayWithCapacity:nchunks];
        for (int i = 0; i < nchunks; i++) [results addObject:[NSMutableArray array]];
        NSLock *lock = [NSLock new];
        __block double done_ms = 0;
        __block int cancelled = 0;
        __block NSString *failure = nil;

        dispatch_apply(nchunks, dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^(size_t ci) {
            @autoreleasepool {
                long long s0 = (long long)ci * per * step;
                long long s1 = MIN((long long)((ci + 1) * per * step), (long long)dur_ms + step);
                if (s0 >= dur_ms) return;
                NSError *e = nil;
                AVAssetReader *r = [[AVAssetReader alloc] initWithAsset:a error:&e];
                if (!r) { [lock lock]; failure = e.localizedDescription; [lock unlock]; return; }
                r.timeRange = CMTimeRangeMake(CMTimeMake(s0, 1000), CMTimeMake(s1 - s0, 1000));
                NSDictionary *set = @{(id)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA),
                                      (id)kCVPixelBufferWidthKey: @(tw), (id)kCVPixelBufferHeightKey: @(th)};
                AVAssetReaderTrackOutput *o = [AVAssetReaderTrackOutput assetReaderTrackOutputWithTrack:v outputSettings:set];
                o.alwaysCopiesSampleData = NO;
                [r addOutput:o];
                if (![r startReading]) { [lock lock]; failure = r.error.localizedDescription; [lock unlock]; return; }
                VNDetectFaceRectanglesRequest *req = [[VNDetectFaceRectanglesRequest alloc] init];
                NSMutableArray *mine = results[ci];
                long long next = s0;
                CMSampleBufferRef sb;
                while ((sb = [o copyNextSampleBuffer])) {
                    @autoreleasepool {
                        double pts = CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sb)) * 1000.0;
                        if (pts + 1 >= next) {
                            CVPixelBufferRef pb = CMSampleBufferGetImageBuffer(sb);
                            if (pb) {
                                VNImageRequestHandler *h = [[VNImageRequestHandler alloc] initWithCVPixelBuffer:pb orientation:orientation_for(rot) options:@{}];
                                NSMutableArray *faces = [NSMutableArray array];
                                if ([h performRequests:@[req] error:nil]) {
                                    for (VNFaceObservation *f in req.results) {
                                        CGRect b = f.boundingBox;
                                        double cx = b.origin.x + b.size.width / 2, cy = 1.0 - (b.origin.y + b.size.height / 2);
                                        [faces addObject:@[@(round(cx * 1000) / 1000), @(round(cy * 1000) / 1000),
                                                           @(round(b.size.width * 1000) / 1000), @(round(b.size.height * 1000) / 1000)]];
                                    }
                                }
                                [mine addObject:@[@((long long)llround(pts)), faces]];
                            }
                            while (next <= pts + 1) next += step;
                        }
                        [lock lock];
                        done_ms += 1000.0 / 30.0;
                        double p = MIN(99.0, done_ms / MAX(dur_ms, 1) * 100.0);
                        if (cb && cb(p, ud)) cancelled = 1;
                        [lock unlock];
                    }
                    CFRelease(sb);
                    if (cancelled) { [r cancelReading]; break; }
                }
            }
        });
        if (cancelled) { *err = dup_str(@"cancelled"); return NULL; }
        if (failure) { *err = dup_str(failure); return NULL; }
        NSMutableArray *all = [NSMutableArray array];
        for (NSArray *c in results) [all addObjectsFromArray:c];
        [all sortUsingComparator:^NSComparisonResult(NSArray *x, NSArray *y) { return [x[0] compare:y[0]]; }];
        return dup_json(@{@"stepMs": @((int)step), @"srcW": @((int)up.width), @"srcH": @((int)up.height), @"samples": all});
    }
}

// ---------------------------------------------------------------- thumbnails

char *cn_thumbnail(const char *path, double t_ms, int out_h, double cx, const char *out_path) {
    @autoreleasepool {
        AVURLAsset *a = open_asset(path);
        AVAssetImageGenerator *g = [[AVAssetImageGenerator alloc] initWithAsset:a];
        g.appliesPreferredTrackTransform = YES;
        g.requestedTimeToleranceBefore = CMTimeMakeWithSeconds(0.6, 600);
        g.requestedTimeToleranceAfter = CMTimeMakeWithSeconds(0.6, 600);
        g.maximumSize = CGSizeMake(1280, 1280);
        NSError *e = nil;
        CGImageRef img = [g copyCGImageAtTime:CMTimeMakeWithSeconds(t_ms / 1000.0, 600) actualTime:NULL error:&e];
        if (!img) return dup_str(e.localizedDescription ?: @"no frame");
        double W = CGImageGetWidth(img), H = CGImageGetHeight(img);
        double cw = MIN(W, H * 9.0 / 16.0);
        double x = MAX(0, MIN(W - cw, cx * W - cw / 2));
        CGImageRef crop = CGImageCreateWithImageInRect(img, CGRectMake(x, 0, cw, H));
        int oh = out_h, ow = (int)(oh * cw / H);
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        CGContextRef ctx = CGBitmapContextCreate(NULL, ow, oh, 8, 0, cs, (CGBitmapInfo)kCGImageAlphaNoneSkipLast);
        CGContextSetInterpolationQuality(ctx, kCGInterpolationHigh);
        CGContextDrawImage(ctx, CGRectMake(0, 0, ow, oh), crop);
        CGImageRef outImg = CGBitmapContextCreateImage(ctx);
        NSURL *u = [NSURL fileURLWithPath:[NSString stringWithUTF8String:out_path]];
        CGImageDestinationRef d = CGImageDestinationCreateWithURL((__bridge CFURLRef)u, CFSTR("public.jpeg"), 1, NULL);
        CGImageDestinationAddImage(d, outImg, (__bridge CFDictionaryRef)@{(id)kCGImageDestinationLossyCompressionQuality: @0.82});
        BOOL ok = CGImageDestinationFinalize(d);
        CFRelease(d); CGImageRelease(outImg); CGContextRelease(ctx); CGColorSpaceRelease(cs); CGImageRelease(crop); CGImageRelease(img);
        return ok ? NULL : dup_str(@"could not write thumbnail");
    }
}

// ---------------------------------------------------------------- export

static CGColorRef make_color(NSString *hex, CGColorSpaceRef cs) {
    NSString *h = [hex hasPrefix:@"#"] ? [hex substringFromIndex:1] : hex;
    unsigned v = 0;
    [[NSScanner scannerWithString:h] scanHexInt:&v];
    CGFloat r, g, b, al = 1;
    if (h.length >= 8) { r = (v >> 24) & 255; g = (v >> 16) & 255; b = (v >> 8) & 255; al = (v & 255) / 255.0; }
    else { r = (v >> 16) & 255; g = (v >> 8) & 255; b = v & 255; }
    CGFloat c[4] = {r / 255, g / 255, b / 255, al};
    return CGColorCreate(cs, c);
}

@interface CNCaptions : NSObject
@property (nonatomic) NSDictionary *style;
@property (nonatomic) NSArray *chunks;
@property (nonatomic) int outW, outH;
@property (nonatomic) NSMutableDictionary<NSNumber *, CIImage *> *cache;
@end

@implementation CNCaptions
- (instancetype)initWithStyle:(NSDictionary *)s chunks:(NSArray *)c w:(int)w h:(int)h {
    if ((self = [super init])) { _style = s; _chunks = c; _outW = w; _outH = h; _cache = [NSMutableDictionary dictionary]; }
    return self;
}

- (CIImage *)overlayAtMs:(double)t {
    NSInteger ci = -1;
    for (NSInteger i = 0; i < (NSInteger)_chunks.count; i++) {
        NSDictionary *c = _chunks[i];
        if (t >= [c[@"t0"] doubleValue] && t < [c[@"t1"] doubleValue]) { ci = i; break; }
    }
    if (ci < 0) return nil;
    NSArray *words = _chunks[ci][@"words"];
    NSInteger active = 0;
    for (NSInteger i = 0; i < (NSInteger)words.count; i++) if ([words[i][1] doubleValue] <= t) active = i;
    NSNumber *key = @(ci * 1000 + active);
    @synchronized (self) {
        CIImage *hit = _cache[key];
        if (hit) return hit;
        if (_cache.count > 96) [_cache removeAllObjects];
    }
    CIImage *img = [self renderWords:words active:active];
    @synchronized (self) { if (img) _cache[key] = img; }
    return img;
}

- (CIImage *)renderWords:(NSArray *)words active:(NSInteger)active {
    NSDictionary *s = _style;
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    double W = _outW, H = _outH;
    double size = [s[@"size"] doubleValue] * W;
    double strokeW = [s[@"strokeW"] doubleValue] * W;
    double maxW = [s[@"maxW"] doubleValue] * W;
    BOOL upper = [s[@"upper"] boolValue];
    BOOL shadow = [s[@"shadow"] boolValue];
    NSString *bgHex = [s[@"bg"] isKindOfClass:[NSString class]] ? s[@"bg"] : nil;

    NSString *fontName = s[@"font"];
    if ([fontName isEqualToString:@"Helvetica Neue"]) fontName = [s[@"bold"] boolValue] ? @"HelveticaNeue-Bold" : @"HelveticaNeue";
    CTFontRef font = CTFontCreateWithName((__bridge CFStringRef)fontName, size, NULL);
    CGColorRef fill = make_color(s[@"fill"], cs), accent = make_color(s[@"accent"], cs), stroke = make_color(s[@"stroke"], cs);

    // words -> CTLines + widths
    NSMutableArray *lines = [NSMutableArray array];
    NSMutableArray *widths = [NSMutableArray array];
    for (NSInteger i = 0; i < (NSInteger)words.count; i++) {
        NSString *txt = words[i][0];
        if (upper) txt = txt.uppercaseString;
        NSDictionary *at = @{(__bridge id)kCTFontAttributeName: (__bridge id)font,
                             (__bridge id)kCTForegroundColorFromContextAttributeName: @YES};
        CFAttributedStringRef as = CFAttributedStringCreate(NULL, (__bridge CFStringRef)txt, (__bridge CFDictionaryRef)at);
        CTLineRef l = CTLineCreateWithAttributedString(as);
        CFRelease(as);
        [lines addObject:(__bridge_transfer id)l];
        [widths addObject:@(CTLineGetTypographicBounds(l, NULL, NULL, NULL))];
    }
    double space = size * 0.28;
    double asc = CTFontGetAscent(font), desc = CTFontGetDescent(font);
    double lineH = (asc + desc) * 1.12;

    // greedy wrap
    NSMutableArray<NSMutableArray *> *rows = [NSMutableArray arrayWithObject:[NSMutableArray array]];
    double rowW = 0;
    for (NSInteger i = 0; i < (NSInteger)words.count; i++) {
        double w = [widths[i] doubleValue];
        if (rowW > 0 && rowW + space + w > maxW) { [rows addObject:[NSMutableArray array]]; rowW = 0; }
        [rows.lastObject addObject:@(i)];
        rowW += (rowW > 0 ? space : 0) + w;
    }
    double padX = size * 0.45, padY = size * 0.2;
    double blockH = ceil(rows.count * lineH + padY * 2 + strokeW * 2 + size * 0.3);
    int bw = _outW, bh = (int)blockH;
    CGContextRef ctx = CGBitmapContextCreate(NULL, bw, bh, 8, 0, cs, (CGBitmapInfo)kCGImageAlphaPremultipliedLast);
    CGContextSetLineJoin(ctx, kCGLineJoinRound);
    CGContextSetTextMatrix(ctx, CGAffineTransformIdentity);

    for (NSInteger r = 0; r < (NSInteger)rows.count; r++) {
        NSArray *row = rows[r];
        double total = 0;
        for (NSNumber *n in row) total += [widths[n.integerValue] doubleValue];
        total += space * (row.count - 1);
        double x = (bw - total) / 2.0;
        // CG origin is bottom-left: first row at the top
        double baseline = bh - padY - size * 0.15 - (r + 0.0) * lineH - asc * 1.04;
        if (bgHex) {
            CGColorRef bg = make_color(bgHex, cs);
            CGContextSetFillColorWithColor(ctx, bg);
            CGRect box = CGRectMake(x - padX, baseline - desc - padY * 0.3, total + padX * 2, asc + desc + padY * 0.8);
            CGPathRef p = CGPathCreateWithRoundedRect(box, size * 0.3, size * 0.3, NULL);
            CGContextAddPath(ctx, p); CGContextFillPath(ctx);
            CGPathRelease(p); CGColorRelease(bg);
        }
        for (NSNumber *n in row) {
            NSInteger i = n.integerValue;
            CTLineRef l = (__bridge CTLineRef)lines[i];
            CGContextSetTextPosition(ctx, x, baseline);
            if (shadow) {
                CGColorRef sh = CGColorCreate(cs, (CGFloat[]){0, 0, 0, 0.65});
                CGContextSetShadowWithColor(ctx, CGSizeMake(0, -size * 0.05), size * 0.14, sh);
                CGColorRelease(sh);
            }
            if (strokeW > 0) {
                CGContextSetTextDrawingMode(ctx, kCGTextStroke);
                CGContextSetStrokeColorWithColor(ctx, stroke);
                CGContextSetLineWidth(ctx, strokeW * 2);
                CTLineDraw(l, ctx);
                CGContextSetTextPosition(ctx, x, baseline);
            }
            CGContextSetTextDrawingMode(ctx, kCGTextFill);
            CGContextSetFillColorWithColor(ctx, (i == active) ? accent : fill);
            CTLineDraw(l, ctx);
            if (shadow) CGContextSetShadowWithColor(ctx, CGSizeZero, 0, NULL);
            x += [widths[i] doubleValue] + space;
        }
    }
    CGImageRef cg = CGBitmapContextCreateImage(ctx);
    CIImage *ci = [CIImage imageWithCGImage:cg];
    // vertical placement: style.y is the block centre measured from the top
    double cy = [s[@"y"] doubleValue] * H;
    double oy = H - cy - bh / 2.0;
    ci = [ci imageByApplyingTransform:CGAffineTransformMakeTranslation(0, oy)];
    CGImageRelease(cg); CGContextRelease(ctx); CGColorRelease(fill); CGColorRelease(accent); CGColorRelease(stroke);
    CGColorSpaceRelease(cs); CFRelease(font);
    return ci;
}
@end

char *cn_export(const char *job_c, cn_progress cb, void *ud) {
    @autoreleasepool {
        NSData *jd = [[NSString stringWithUTF8String:job_c] dataUsingEncoding:NSUTF8StringEncoding];
        NSDictionary *job = [NSJSONSerialization JSONObjectWithData:jd options:0 error:nil];
        if (!job) return dup_str(@"bad export job");
        NSString *src = job[@"src"], *out = job[@"out"];
        int outW = [job[@"width"] intValue], outH = [job[@"height"] intValue];
        double startMs = [job[@"startMs"] doubleValue], endMs = [job[@"endMs"] doubleValue];
        NSDictionary *path = job[@"path"];
        double pT0 = [path[@"t0"] doubleValue], pStep = MAX(1, [path[@"step"] doubleValue]);
        NSArray *pcx = path[@"cx"], *pcy = path[@"cy"];
        double cropW = [path[@"cropW"] doubleValue], cropH = [path[@"cropH"] doubleValue];
        NSInteger pn = pcx.count;
        if (pn < 2) return dup_str(@"empty camera path");

        AVURLAsset *srcAsset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:src] options:@{AVURLAssetPreferPreciseDurationAndTimingKey: @YES}];
        AVAssetTrack *vt = [[srcAsset tracksWithMediaType:AVMediaTypeVideo] firstObject];
        if (!vt) return dup_str(@"no video track");
        AVAsset *asset = srcAsset;

        // Jump cuts: stitch the kept segments into a composition; `cmap` maps composition time back to
        // source time so the camera path and captions (both in source time) still line up.
        NSArray *segs = [job[@"segments"] isKindOfClass:[NSArray class]] ? job[@"segments"] : nil;
        NSMutableArray *cmap = nil; // [compStartMs, srcStartMs, lenMs]
        BOOL cut = segs.count > 1;
        double totalMs = endMs - startMs;
        if (cut) {
            AVMutableComposition *mc = [AVMutableComposition composition];
            AVMutableCompositionTrack *cv = [mc addMutableTrackWithMediaType:AVMediaTypeVideo preferredTrackID:kCMPersistentTrackID_Invalid];
            AVAssetTrack *at = [[srcAsset tracksWithMediaType:AVMediaTypeAudio] firstObject];
            AVMutableCompositionTrack *ca = at ? [mc addMutableTrackWithMediaType:AVMediaTypeAudio preferredTrackID:kCMPersistentTrackID_Invalid] : nil;
            cmap = [NSMutableArray array];
            double comp = 0;
            for (NSArray *sg in segs) {
                double a = [sg[0] doubleValue], b = [sg[1] doubleValue];
                if (b <= a) continue;
                CMTimeRange r = CMTimeRangeMake(CMTimeMakeWithSeconds(a / 1000.0, 600), CMTimeMakeWithSeconds((b - a) / 1000.0, 600));
                CMTime when = CMTimeMakeWithSeconds(comp / 1000.0, 600);
                NSError *ie = nil;
                if (![cv insertTimeRange:r ofTrack:vt atTime:when error:&ie]) return dup_str(ie.localizedDescription ?: @"could not cut the clip");
                if (ca) [ca insertTimeRange:r ofTrack:at atTime:when error:nil];
                [cmap addObject:@[@(comp), @(a), @(b - a)]];
                comp += (b - a);
            }
            cv.preferredTransform = vt.preferredTransform;
            asset = mc;
            totalMs = comp;
        }

        CNCaptions *caps = nil;
        NSDictionary *cj = [job[@"captions"] isKindOfClass:[NSDictionary class]] ? job[@"captions"] : nil;
        if (cj) caps = [[CNCaptions alloc] initWithStyle:cj[@"style"] chunks:cj[@"chunks"] w:outW h:outH];

        CIFilter *lanczos = nil; // created per frame: filters are not thread safe
        (void)lanczos;

        AVMutableVideoComposition *comp = [AVMutableVideoComposition videoCompositionWithAsset:asset applyingCIFiltersWithHandler:^(AVAsynchronousCIImageFilteringRequest *req) {
            double t = CMTimeGetSeconds(req.compositionTime) * 1000.0;
            if (cmap) {
                for (NSInteger si = (NSInteger)cmap.count - 1; si >= 0; si--) {
                    NSArray *m = cmap[si];
                    if (t >= [m[0] doubleValue] || si == 0) { t = [m[1] doubleValue] + MIN(t - [m[0] doubleValue], [m[2] doubleValue]); break; }
                }
            }
            double f = (t - pT0) / pStep;
            f = MAX(0, MIN((double)(pn - 1), f));
            NSInteger i = (NSInteger)floor(f);
            NSInteger j = MIN(pn - 1, i + 1);
            double u = f - i;
            double cx = [pcx[i] doubleValue] * (1 - u) + [pcx[j] doubleValue] * u;
            double cy = [pcy[i] doubleValue] * (1 - u) + [pcy[j] doubleValue] * u;

            CIImage *srcImg = req.sourceImage;
            CGRect ext = srcImg.extent;
            double W = ext.size.width, H = ext.size.height;
            double cwpx = cropW * W, chpx = cropH * H;
            double x = MAX(0, MIN(W - cwpx, cx * W - cwpx / 2));
            double yTop = MAX(0, MIN(H - chpx, cy * H - chpx / 2));
            double yCI = H - yTop - chpx;
            CIImage *c = [srcImg imageByCroppingToRect:CGRectMake(ext.origin.x + x, ext.origin.y + yCI, cwpx, chpx)];
            c = [c imageByApplyingTransform:CGAffineTransformMakeTranslation(-(ext.origin.x + x), -(ext.origin.y + yCI))];
            double sy = outH / chpx, sx = outW / cwpx;
            CIFilter *lf = [CIFilter filterWithName:@"CILanczosScaleTransform"];
            [lf setValue:c forKey:kCIInputImageKey];
            [lf setValue:@(sy) forKey:kCIInputScaleKey];
            [lf setValue:@(sx / sy) forKey:kCIInputAspectRatioKey];
            CIImage *scaled = lf.outputImage ?: c;
            CIImage *res = [scaled imageByCroppingToRect:CGRectMake(0, 0, outW, outH)];
            if (caps) {
                CIImage *ov = [caps overlayAtMs:t];
                if (ov) res = [[ov imageByCompositingOverImage:res] imageByCroppingToRect:CGRectMake(0, 0, outW, outH)];
            }
            [req finishWithImage:res context:nil];
        }];
        comp.renderSize = CGSizeMake(outW, outH);
        if (vt.nominalFrameRate > 31) comp.frameDuration = CMTimeMake(1, 30);

        NSString *preset = [job[@"codec"] isEqualToString:@"hevc"] ? AVAssetExportPresetHEVCHighestQuality : AVAssetExportPresetHighestQuality;
        if (![[AVAssetExportSession exportPresetsCompatibleWithAsset:asset] containsObject:preset]) preset = AVAssetExportPresetHighestQuality;
        AVAssetExportSession *ex = [[AVAssetExportSession alloc] initWithAsset:asset presetName:preset];
        if (!ex) return dup_str(@"could not create an export session");
        NSURL *outURL = [NSURL fileURLWithPath:out];
        [[NSFileManager defaultManager] removeItemAtURL:outURL error:nil];
        ex.outputURL = outURL;
        ex.outputFileType = AVFileTypeMPEG4;
        ex.videoComposition = comp;
        ex.shouldOptimizeForNetworkUse = YES;
        if (cut) ex.timeRange = CMTimeRangeMake(kCMTimeZero, CMTimeMakeWithSeconds(totalMs / 1000.0, 600));
        else ex.timeRange = CMTimeRangeMake(CMTimeMakeWithSeconds(startMs / 1000.0, 600), CMTimeMakeWithSeconds((endMs - startMs) / 1000.0, 600));

        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        [ex exportAsynchronouslyWithCompletionHandler:^{ dispatch_semaphore_signal(done); }];
        BOOL cancelled = NO;
        while (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 80 * NSEC_PER_MSEC)) != 0) {
            if (cb && cb(ex.progress * 100.0, ud) && !cancelled) { cancelled = YES; [ex cancelExport]; }
        }
        if (ex.status == AVAssetExportSessionStatusCompleted) { if (cb) cb(100.0, ud); return NULL; }
        [[NSFileManager defaultManager] removeItemAtURL:outURL error:nil];
        if (cancelled || ex.status == AVAssetExportSessionStatusCancelled) return dup_str(@"cancelled");
        return dup_str(ex.error.localizedDescription ?: @"export failed");
    }
}


// ---------------------------------------------------------------- player

@interface CNPlayer : NSObject
@property (nonatomic) AVPlayer *player;
@property (nonatomic) AVPlayerItem *item;
@property (nonatomic) AVPlayerItemVideoOutput *out;
@property (nonatomic) CIContext *ci;
@property (nonatomic) CVPixelBufferRef last;
@property (nonatomic) int rot;
@property (nonatomic) BOOL stale;
@end

@implementation CNPlayer
- (void)dealloc { if (_last) CFRelease(_last); }
@end

void *cn_player_new(const char *path) {
    @autoreleasepool {
        CNPlayer *p = [CNPlayer new];
        AVURLAsset *a = open_asset(path);
        AVAssetTrack *v = [[a tracksWithMediaType:AVMediaTypeVideo] firstObject];
        p.rot = v ? rotation_of(v.preferredTransform) : 0;
        p.item = [AVPlayerItem playerItemWithAsset:a];
        p.out = [[AVPlayerItemVideoOutput alloc] initWithPixelBufferAttributes:@{(id)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA)}];
        [p.item addOutput:p.out];
        p.player = [AVPlayer playerWithPlayerItem:p.item];
        p.player.actionAtItemEnd = AVPlayerActionAtItemEndPause;
        p.player.automaticallyWaitsToMinimizeStalling = NO;
        p.ci = [CIContext contextWithOptions:@{kCIContextUseSoftwareRenderer: @NO}];
        p.stale = YES;
        return (void *)CFBridgingRetain(p);
    }
}
void cn_player_free(void *h) { if (h) { CNPlayer *p = (CNPlayer *)CFBridgingRelease(h); [p.player pause]; } }
int cn_player_status(void *h) {
    CNPlayer *p = (__bridge CNPlayer *)h;
    switch (p.item.status) { case AVPlayerItemStatusReadyToPlay: return 1; case AVPlayerItemStatusFailed: return -1; default: return 0; }
}
void cn_player_play(void *h) { [((__bridge CNPlayer *)h).player play]; }
void cn_player_pause(void *h) { [((__bridge CNPlayer *)h).player pause]; }
int cn_player_playing(void *h) { return ((__bridge CNPlayer *)h).player.rate != 0; }
void cn_player_seek(void *h, double ms) {
    CNPlayer *p = (__bridge CNPlayer *)h;
    p.stale = YES;
    [p.player seekToTime:CMTimeMakeWithSeconds(ms / 1000.0, 600) toleranceBefore:kCMTimeZero toleranceAfter:kCMTimeZero];
}
double cn_player_time(void *h) {
    CNPlayer *p = (__bridge CNPlayer *)h;
    double s = CMTimeGetSeconds(p.player.currentTime);
    return isfinite(s) ? s * 1000.0 : 0;
}

int cn_player_frame(void *h, double cx, double cy, double cw, double ch, int ow, int oh, unsigned char *rgba, int force) {
    @autoreleasepool {
        CNPlayer *p = (__bridge CNPlayer *)h;
        BOOL playing = p.player.rate != 0;
        CMTime t = playing ? [p.out itemTimeForHostTime:CACurrentMediaTime()] : p.player.currentTime;
        BOOL fresh = NO;
        if ([p.out hasNewPixelBufferForItemTime:t]) {
            CVPixelBufferRef pb = [p.out copyPixelBufferForItemTime:t itemTimeForDisplay:NULL];
            if (pb) { if (p.last) CFRelease(p.last); p.last = pb; fresh = YES; p.stale = NO; }
        }
        if (!p.last || (p.stale && !fresh) || (!fresh && !force)) return 0;
        CIImage *img = [CIImage imageWithCVPixelBuffer:p.last];
        CGImagePropertyOrientation ori = p.rot == 90 ? kCGImagePropertyOrientationRight : p.rot == 270 ? kCGImagePropertyOrientationLeft : p.rot == 180 ? kCGImagePropertyOrientationDown : kCGImagePropertyOrientationUp;
        if (p.rot) img = [img imageByApplyingCGOrientation:ori];
        CGRect ext = img.extent;
        double W = ext.size.width, H = ext.size.height;
        double cwpx = cw * W, chpx = ch * H;
        double x = MAX(0, MIN(W - cwpx, cx * W - cwpx / 2));
        double yTop = MAX(0, MIN(H - chpx, cy * H - chpx / 2));
        double yCI = H - yTop - chpx;
        CIImage *c = [img imageByCroppingToRect:CGRectMake(ext.origin.x + x, ext.origin.y + yCI, cwpx, chpx)];
        c = [c imageByApplyingTransform:CGAffineTransformMakeTranslation(-(ext.origin.x + x), -(ext.origin.y + yCI))];
        c = [c imageByApplyingTransform:CGAffineTransformMakeScale(ow / cwpx, oh / chpx)];
        static CGColorSpaceRef cs; static dispatch_once_t once;
        dispatch_once(&once, ^{ cs = CGColorSpaceCreateWithName(kCGColorSpaceSRGB); });
        [p.ci render:c toBitmap:rgba rowBytes:ow * 4 bounds:CGRectMake(0, 0, ow, oh) format:kCIFormatRGBA8 colorSpace:cs];
        return 1;
    }
}
