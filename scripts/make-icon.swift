// Renders the ClipCraft app icon (1024x1024) with CoreGraphics.
// Usage: swift scripts/make-icon.swift src-tauri/icons/icon-1024.png
import AppKit
import CoreGraphics

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon-1024.png"
let S = 1024
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
guard let ctx = CGContext(data: nil, width: S, height: S, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fatalError("ctx") }

func color(_ r: Double, _ g: Double, _ b: Double, _ a: Double = 1) -> CGColor {
    CGColor(colorSpace: cs, components: [r / 255, g / 255, b / 255, a])!
}

// macOS icon grid: 824pt tile centred in the 1024pt canvas
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let shape = CGPath(roundedRect: tile, cornerWidth: 186, cornerHeight: 186, transform: nil)

// soft drop shadow under the tile
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -14), blur: 26, color: color(0, 20, 70, 0.28))
ctx.addPath(shape)
ctx.setFillColor(color(40, 100, 200))
ctx.fillPath()
ctx.restoreGState()

// violet gradient fill with a top sheen
ctx.saveGState()
ctx.addPath(shape)
ctx.clip()
let grad = CGGradient(colorsSpace: cs, colors: [color(94, 168, 240), color(38, 98, 200), color(30, 70, 168)] as CFArray, locations: [0, 0.5, 1])!
ctx.drawLinearGradient(grad, start: CGPoint(x: 260, y: 924), end: CGPoint(x: 780, y: 100), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
let sheen = CGGradient(colorsSpace: cs, colors: [color(255, 255, 255, 0.26), color(255, 255, 255, 0)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(sheen, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 560), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
ctx.restoreGState()

// thin inner edge
ctx.saveGState()
ctx.addPath(CGPath(roundedRect: tile.insetBy(dx: 1.5, dy: 1.5), cornerWidth: 184, cornerHeight: 184, transform: nil))
ctx.setStrokeColor(color(255, 255, 255, 0.35))
ctx.setLineWidth(3)
ctx.strokePath()
ctx.restoreGState()

let c = CGPoint(x: 512, y: 512)

// the 16:9 source frame, ghosted behind the crop
let wide = CGRect(x: c.x - 290, y: c.y - 163, width: 580, height: 326)
ctx.saveGState()
ctx.setStrokeColor(color(255, 255, 255, 0.34))
ctx.setLineWidth(22)
ctx.addPath(CGPath(roundedRect: wide, cornerWidth: 46, cornerHeight: 46, transform: nil))
ctx.strokePath()
ctx.restoreGState()

// the 9:16 crop: solid, with a play glyph
let tall = CGRect(x: c.x - 150, y: c.y - 267, width: 300, height: 534)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(8, 30, 90, 0.4))
ctx.addPath(CGPath(roundedRect: tall, cornerWidth: 62, cornerHeight: 62, transform: nil))
ctx.setFillColor(color(255, 255, 255))
ctx.fillPath()
ctx.restoreGState()

// play triangle in the crop (violet)
ctx.saveGState()
ctx.setFillColor(color(38, 98, 200))
let tri = CGMutablePath()
tri.move(to: CGPoint(x: c.x - 38, y: c.y + 66))
tri.addLine(to: CGPoint(x: c.x + 78, y: c.y))
tri.addLine(to: CGPoint(x: c.x - 38, y: c.y - 66))
tri.closeSubpath()
ctx.addPath(tri.copy(using: [CGAffineTransform(a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0)])!)
ctx.fillPath()
ctx.restoreGState()

// coral caption bar near the bottom of the crop, hinting at burned-in captions
ctx.saveGState()
ctx.setFillColor(color(255, 138, 92))
ctx.addPath(CGPath(roundedRect: CGRect(x: c.x - 92, y: c.y - 205, width: 184, height: 34), cornerWidth: 17, cornerHeight: 17, transform: nil))
ctx.fillPath()
ctx.restoreGState()

let img = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: img)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
