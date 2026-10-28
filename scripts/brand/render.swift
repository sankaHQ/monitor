// Draws Sanka Monitor's brand bitmaps: the Lens app icon (two 1024 masters for
// `tauri icon`), the menu bar template icons and the DMG background.
//
//   swift scripts/brand/render.swift <out-dir>
//
// Then, in apps/agent, run `npx tauri icon` on each master into a scratch
// directory and keep icon.icns from the glass one and icon.ico and icon.png
// from the flat one; the DMG background and tray templates are copied as is.
//
// The Lens is an open ring with a center dot, in black on a light pink tile.
// It comes in two designs. On the Mac (icon.icns, the DMG) it is glass:
// glossy black on a pink glass tile (#fcf5f2 → #f1dfd8), in the Liquid Glass
// style of macOS 26, with a sheen across the top, a bright rim, and a lit,
// shadowed glyph (icon-1024-glass.png). Everywhere else (icon.ico, icon.png,
// the in-app and README logo, the web) it is flat: solid #16151a on a solid
// #f8ece7 tile, with no sheen, rim, gradients or shadows (icon-1024-flat.png).
// Template icons are black with alpha only, so macOS tints
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
let sRGB = CGColorSpace(name: CGColorSpace.sRGB)!
func rgb(_ hex: Int, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat(hex >> 16 & 0xff) / 255, green: CGFloat(hex >> 8 & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
}
func gradient(_ colors: [CGColor], _ locations: [CGFloat]) -> CGGradient {
    CGGradient(colorsSpace: sRGB, colors: colors as CFArray, locations: locations)!
}

/// The Lens glyph as one filled outline (ring and dot), in the 1024 box.
func lensOutline() -> CGPath {
    let ring = CGMutablePath()
    ring.addArc(center: CGPoint(x: 512, y: 512), radius: 240, startAngle: -.pi / 4, endAngle: 0, clockwise: true)
    let outline = CGMutablePath()
    outline.addPath(ring.copy(strokingWithWidth: 76, lineCap: .round, lineJoin: .round, miterLimit: 10))
    outline.addEllipse(in: CGRect(x: 512 - 80, y: 512 - 80, width: 160, height: 160))
    return outline
}

// Glass: the macOS app icon.
save(bitmap(1024, 1024) { cg in
    let rect = CGRect(x: 100, y: 100, width: 824, height: 824)
    let tile = CGPath(roundedRect: rect, cornerWidth: 185, cornerHeight: 185, transform: nil)
    // Drop shadow under the tile.
    cg.saveGState()
    cg.setShadow(offset: CGSize(width: 0, height: 12), blur: 30, color: rgb(0x3a2620, 0.22))
    cg.addPath(tile)
    cg.setFillColor(rgb(0xf8efeb))
    cg.fillPath()
    cg.restoreGState()

    cg.saveGState()
    cg.addPath(tile)
    cg.clip()
    // Pink body, lighter at the top.
    cg.drawLinearGradient(gradient([rgb(0xfcf5f2), rgb(0xf8ece7), rgb(0xf1dfd8)], [0, 0.55, 1]),
                          start: CGPoint(x: 512, y: 100), end: CGPoint(x: 512, y: 924), options: [])
    // Sheen: a soft white bloom across the upper half.
    cg.drawRadialGradient(gradient([rgb(0xffffff, 0.6), rgb(0xffffff, 0)], [0, 1]),
                          startCenter: CGPoint(x: 512, y: 150), startRadius: 0,
                          endCenter: CGPoint(x: 512, y: 150), endRadius: 520, options: [])
    // Warm glow pooling at the bottom, as light through tinted glass.
    cg.drawRadialGradient(gradient([rgb(0xefd6ce, 0.55), rgb(0xefd6ce, 0)], [0, 1]),
                          startCenter: CGPoint(x: 512, y: 960), startRadius: 0,
                          endCenter: CGPoint(x: 512, y: 960), endRadius: 460, options: [])
    // Rim: bright along the top edge, a faint darker line along the bottom.
    cg.addPath(tile)
    cg.setLineWidth(10)
    cg.replacePathWithStrokedPath()
    cg.clip()
    cg.drawLinearGradient(gradient([rgb(0xffffff, 1), rgb(0xffffff, 0.15), rgb(0xc9a89e, 0.35)], [0, 0.5, 1]),
                          start: CGPoint(x: 512, y: 100), end: CGPoint(x: 512, y: 924), options: [])
    cg.restoreGState()

    let glyph = lensOutline()
    // The glyph's shadow on the glass.
    cg.saveGState()
    cg.setShadow(offset: CGSize(width: 0, height: 18), blur: 34, color: rgb(0x2a1a14, 0.30))
    cg.addPath(glyph)
    cg.setFillColor(rgb(0x16151a))
    cg.fillPath()
    cg.restoreGState()
    // Glossy black: a gradient body, a lit upper edge, a specular streak.
    cg.saveGState()
    cg.addPath(glyph)
    cg.clip()
    cg.drawLinearGradient(gradient([rgb(0x3a3840), rgb(0x16151a), rgb(0x050506)], [0, 0.5, 1]),
                          start: CGPoint(x: 512, y: 236), end: CGPoint(x: 512, y: 788), options: [])
    cg.drawRadialGradient(gradient([rgb(0xffffff, 0.42), rgb(0xffffff, 0)], [0, 1]),
                          startCenter: CGPoint(x: 430, y: 300), startRadius: 0,
                          endCenter: CGPoint(x: 430, y: 300), endRadius: 230, options: [])
    cg.drawRadialGradient(gradient([rgb(0xffffff, 0.38), rgb(0xffffff, 0)], [0, 1]),
                          startCenter: CGPoint(x: 490, y: 470), startRadius: 0,
                          endCenter: CGPoint(x: 490, y: 470), endRadius: 70, options: [])
    // Inner edge light: the outline offset down shows a thin bright line on top.
    cg.addPath(glyph)
    cg.setLineWidth(6)
    cg.setStrokeColor(rgb(0xffffff, 0.18))
    cg.strokePath()
    cg.restoreGState()
}, "icon-1024-glass.png")

// Flat: the same tile and glyph in two solid colors, for Windows, the in-app
// logo (apps/agent/src/assets/lens.svg), the README and the web.
save(bitmap(1024, 1024) { cg in
    cg.addPath(CGPath(roundedRect: CGRect(x: 100, y: 100, width: 824, height: 824),
                      cornerWidth: 185, cornerHeight: 185, transform: nil))
    cg.setFillColor(rgb(0xf8ece7))
    cg.fillPath()
    cg.addPath(lensOutline())
    cg.setFillColor(rgb(0x16151a))
    cg.fillPath()
}, "icon-1024-flat.png")

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
    cg.setStrokeColor(rgb(0x16151a, 0.55))
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
