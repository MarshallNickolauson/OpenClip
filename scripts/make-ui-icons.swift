// Renders the UI glyphs (white on transparent, 96px) from SVG sources with AppKit.
// Usage: swift scripts/make-ui-icons.swift app/assets/icons
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icons"
let stroke = "fill=\"none\" stroke=\"white\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\""
let fill = "fill=\"white\" stroke=\"none\""
let icons: [String: String] = [
  "undo": "<g \(stroke)><path d=\"M9 14L4 9l5-5\"/><path d=\"M4 9h10a6 6 0 010 12h-3\"/></g>",
  "redo": "<g \(stroke)><path d=\"M15 14l5-5-5-5\"/><path d=\"M20 9H10a6 6 0 000 12h3\"/></g>",
  "play": "<path \(fill) d=\"M7 4l13 8-13 8z\"/>",
  "pause": "<path \(fill) d=\"M7 4h4v16H7zM13 4h4v16h-4z\"/>",
  "skip_back": "<g \(stroke)><path d=\"M6 5v14M18 6l-9 6 9 6z\"/></g>",
  "skip_next": "<g \(stroke)><path d=\"M18 5v14M6 6l9 6-9 6z\"/></g>",
  "plus": "<g \(stroke)><path d=\"M12 5v14M5 12h14\"/></g>",
  "x": "<g \(stroke)><path d=\"M6 6l12 12M18 6L6 18\"/></g>",
  "check": "<path fill=\"none\" stroke=\"white\" stroke-width=\"3\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"M5 12.5l4.5 4.5L19 7.5\"/>",
  "back": "<g \(stroke)><path d=\"M15 6l-6 6 6 6\"/></g>",
  "upload": "<g \(stroke)><path d=\"M12 16V4M7 9l5-5 5 5M5 20h14\"/></g>",
  "clip": "<g \(stroke)><rect x=\"7\" y=\"2.5\" width=\"10\" height=\"19\" rx=\"2.5\"/><path d=\"M11 9.5l3 2.5-3 2.5z\"/></g>",
  "caption": "<g \(stroke)><rect x=\"3\" y=\"5\" width=\"18\" height=\"14\" rx=\"3\"/><path d=\"M7 11h4M13 11h4M7 15h7\"/></g>",
  "frame": "<g \(stroke)><path d=\"M4 8V5a1 1 0 011-1h3M16 4h3a1 1 0 011 1v3M20 16v3a1 1 0 01-1 1h-3M8 20H5a1 1 0 01-1-1v-3\"/><circle cx=\"12\" cy=\"11\" r=\"2.6\"/><path d=\"M7.5 17c.8-2 2.4-3 4.5-3s3.7 1 4.5 3\"/></g>",
  "zoom_in": "<g \(stroke)><circle cx=\"11\" cy=\"11\" r=\"6\"/><path d=\"M21 21l-5-5M11 8v6M8 11h6\"/></g>",
  "zoom_out": "<g \(stroke)><circle cx=\"11\" cy=\"11\" r=\"6\"/><path d=\"M21 21l-5-5M8 11h6\"/></g>",
  "folder": "<g \(stroke)><path d=\"M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z\"/></g>",
  "drop": "<g \(stroke)><rect x=\"7\" y=\"2.5\" width=\"10\" height=\"19\" rx=\"2.5\"/><path d=\"M11 9.5l3 2.5-3 2.5z\"/></g>",
  "xlogo": "<path \(fill) d=\"M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z\"/>",
  "github": "<path \(fill) d=\"M12 .5a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.37-3.87-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.76 2.7 1.25 3.36.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.62 1.59.23 2.76.11 3.05.74.81 1.18 1.83 1.18 3.09 0 4.42-2.69 5.39-5.25 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .5Z\"/>",
  "chev_down": "<g \(stroke)><path d=\"M6 9l6 6 6-6\"/></g>",
]
try? FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)
let S = 96
for (name, body) in icons {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"\(S)\" height=\"\(S)\" viewBox=\"0 0 24 24\">\(body)</svg>"
    guard let img = NSImage(data: svg.data(using: .utf8)!) else { print("failed \(name)"); continue }
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: S, pixelsHigh: S, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    img.draw(in: NSRect(x: 0, y: 0, width: S, height: S))
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: "\(out)/\(name).png"))
}
print("wrote \(icons.count) icons")
