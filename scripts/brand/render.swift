// Draws Sanka Monitor's brand bitmaps: the Lens app icon (1024 master for
// `tauri icon`), the menu bar template icons and the DMG background.
//
//   swift scripts/brand/render.swift <out-dir>
//
// The Lens is an open ring with a center dot on a violet tile (Sanka violet
// #472cf5 → #3a20d8). Template icons are black with alpha only, so macOS tints
// them for light and dark menu bars; the state shows in the shape: a badge in
// the ring's gap when something needs attention, a hollow dot when paused.

import AppKit

let out = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ".")
try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

func bitmap(_ width: Int, _ height: Int, _ draw: (CGContext) -> Void) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    let cg = context.cgContext
    // Draw in a top-left origin, like the SVG mockups.
    cg.translateBy(x: 0, y: CGFloat(height))
    cg.scaleBy(x: 1, y: -1)
    draw(cg)
    NSGraphicsContext.restoreGraphicsState()
    return rep
}

func save(_ rep: NSBitmapImageRep, _ name: String, dpi: CGFloat = 72) {
    rep.size = NSSize(width: CGFloat(rep.pixelsWide) * 72 / dpi, height: CGFloat(rep.pixelsHigh) * 72 / dpi)
    let data = rep.representation(using: .png, properties: [:])!
    try! data.write(to: out.appendingPathComponent(name))
    print(name)
}

let violet = CGColor(srgbRed: 0x47 / 255, green: 0x2c / 255, blue: 0xf5 / 255, alpha: 1)
let violetTop = CGColor(srgbRed: 0x5a / 255, green: 0x40 / 255, blue: 0xff / 255, alpha: 1)
let violetDeep = CGColor(srgbRed: 0x3a / 255, green: 0x20 / 255, blue: 0xd8 / 255, alpha: 1)

/// The ring: an arc from the gap at the upper right, around, back to the right.
/// Geometry in a 1024 box (center 512, radius 240); `scale` maps it to the target.
func lens(_ cg: CGContext, scale: CGFloat, origin: CGPoint, color: CGColor, ring: CGFloat, dot: CGFloat,
          hollowDot: Bool = false, badge: Bool = false) {
    cg.saveGState()
    cg.translateBy(x: origin.x, y: origin.y)
    cg.scaleBy(x: scale, y: scale)
    let center = CGPoint(x: 512, y: 512)
    cg.setStrokeColor(color)
    cg.setFillColor(color)
    cg.setLineCap(.round)
    cg.setLineWidth(ring)
    // Gap from -45° (upper right) to 0° (right), in the flipped space.
    cg.addArc(center: center, radius: 240, startAngle: -.pi / 4, endAngle: 0, clockwise: true)
    cg.strokePath()
    if hollowDot {
        cg.setLineWidth(ring * 0.85)
        cg.strokeEllipse(in: CGRect(x: 512 - dot + ring * 0.42, y: 512 - dot + ring * 0.42,
                                    width: (dot - ring * 0.42) * 2, height: (dot - ring * 0.42) * 2))
    } else {
        cg.fillEllipse(in: CGRect(x: 512 - dot, y: 512 - dot, width: dot * 2, height: dot * 2))
    }
    if badge {
        // Cut a clear margin out of the ring, then draw the badge in it.
        let c = CGPoint(x: 728, y: 296), r: CGFloat = 104, margin: CGFloat = 52
        cg.setBlendMode(.clear)
        cg.fillEllipse(in: CGRect(x: c.x - r - margin, y: c.y - r - margin, width: (r + margin) * 2, height: (r + margin) * 2))
        cg.setBlendMode(.normal)
        cg.fillEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2))
    }
    cg.restoreGState()
}

// App icon, 1024 master on the macOS icon grid (an 824 tile with 100 margins).
save(bitmap(1024, 1024) { cg in
    let tile = CGPath(roundedRect: CGRect(x: 100, y: 100, width: 824, height: 824),
                      cornerWidth: 185, cornerHeight: 185, transform: nil)
    cg.saveGState()
    cg.setShadow(offset: CGSize(width: 0, height: 10), blur: 28,
                 color: CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 0.28))
    cg.addPath(tile)
    cg.setFillColor(violet)
    cg.fillPath()
    cg.restoreGState()
    cg.saveGState()
    cg.addPath(tile)
    cg.clip()
    let gradient = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
                              colors: [violetTop, violetDeep] as CFArray, locations: [0, 1])!
    cg.drawLinearGradient(gradient, start: CGPoint(x: 100, y: 100), end: CGPoint(x: 924, y: 924), options: [])
    cg.restoreGState()
    lens(cg, scale: 1, origin: .zero, color: CGColor(gray: 1, alpha: 1), ring: 72, dot: 78)
}, "icon-1024.png")

// Menu bar templates: 18pt drawn at 2x (36px); the tray shows them 18pt high.
let black = CGColor(gray: 0, alpha: 1)
for (name, hollow, badge) in [("trayTemplate", false, false), ("trayAttentionTemplate", false, true),
                              ("trayPausedTemplate", true, false)] {
    for (suffix, size) in [("", 18), ("@2x", 36)] {
        save(bitmap(size, size) { cg in
            // The 1024 geometry's ring spans 512±276 with the stroke; fit it to the canvas.
            let scale = CGFloat(size) / 640
            let shift = (CGFloat(size) - 1024 * scale) / 2
            lens(cg, scale: scale, origin: CGPoint(x: shift, y: shift), color: black,
                 ring: 76, dot: 92, hollowDot: hollow, badge: badge)
        }, "\(name)\(suffix).png", dpi: suffix.isEmpty ? 72 : 144)
    }
}

// DMG background, 640×400 points at 2x. Tauri places the app at (190, 210) and
// Applications at (450, 210) (tauri.conf.json bundle.macOS.dmg).
save(bitmap(1280, 800) { cg in
    cg.scaleBy(x: 2, y: 2)
    cg.setFillColor(CGColor(srgbRed: 0xfa / 255, green: 0xfa / 255, blue: 0xf8 / 255, alpha: 1))
    cg.fill(CGRect(x: 0, y: 0, width: 640, height: 400))
    cg.setFillColor(CGColor(srgbRed: 0x16 / 255, green: 0x15 / 255, blue: 0x1a / 255, alpha: 0.07))
    var y: CGFloat = 10
    while y < 400 {
        var x: CGFloat = 10
        while x < 640 { cg.fillEllipse(in: CGRect(x: x - 0.6, y: y - 0.6, width: 1.2, height: 1.2)); x += 20 }
        y += 20
    }
    // Arrow between the two icons.
    cg.setStrokeColor(violet)
    cg.setLineWidth(2.5)
    cg.setLineCap(.round)
    cg.setLineJoin(.round)
    cg.move(to: CGPoint(x: 280, y: 210)); cg.addLine(to: CGPoint(x: 360, y: 210))
    cg.move(to: CGPoint(x: 350, y: 202)); cg.addLine(to: CGPoint(x: 360, y: 210)); cg.addLine(to: CGPoint(x: 350, y: 218))
    cg.strokePath()
    let ink = NSColor(srgbRed: 0x16 / 255, green: 0x15 / 255, blue: 0x1a / 255, alpha: 1)
    let caption = NSColor(srgbRed: 0x16 / 255, green: 0x15 / 255, blue: 0x1a / 255, alpha: 0.6)
    func text(_ string: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, y: CGFloat) {
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color, .paragraphStyle: style,
        ]
        NSString(string: string).draw(in: CGRect(x: 0, y: y, width: 640, height: size * 1.6), withAttributes: attributes)
    }
    // Text draws in the flipped context through AppKit.
    NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: true)
    text("Install Sanka Monitor", size: 20, weight: .semibold, color: ink, y: 40)
    text("Drag the app into Applications", size: 13, weight: .regular, color: caption, y: 70)
    text("Signed by Sanka, Inc. · Notarized by Apple", size: 11, weight: .regular, color: caption, y: 360)
}, "background.png", dpi: 144)
