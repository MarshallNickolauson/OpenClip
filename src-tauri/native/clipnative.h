// ClipCraft native media layer (AVFoundation, Vision, Core Image, CoreText).
// A thin C ABI the Rust side calls; everything heavy runs on Apple's hardware decoders / encoders.
#ifndef CLIPNATIVE_H
#define CLIPNATIVE_H
#include <stddef.h>

// Called with 0..100; return non-zero to cancel.
typedef int (*cn_progress)(double pct, void *ud);

// JSON: {"durationMs","width","height","fps","hasAudio"} or {"error":"..."}.
char *cn_probe(const char *path);

// Mono 16 kHz float32 PCM (what whisper wants). NULL on failure with *err set. Free with cn_free.
float *cn_decode_audio(const char *path, size_t *n_samples, cn_progress cb, void *ud, char **err);

// JSON FaceTrack: {"stepMs","srcW","srcH","samples":[[t,[[cx,cy,w,h],...]],...]}.
char *cn_scan_faces(const char *path, int step_ms, cn_progress cb, void *ud, char **err);

// 9:16 JPEG of the frame at t_ms, cropped around cx (0..1). Returns NULL on success, else an error string.
char *cn_thumbnail(const char *path, double t_ms, int out_h, double cx, const char *out_path);

// Render one clip. `job` is JSON (see export.rs). Returns NULL on success, else an error string.
char *cn_export(const char *job, cn_progress cb, void *ud);

// Turns off AppKit's Dictation / Emoji & Symbols / AutoFill additions to the Edit menu.
void cn_trim_edit_menu(void);

void cn_free(void *p);
#endif
