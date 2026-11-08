import AppKit
import CoreText

/// Menu-bar cat-ear logo (+ optional stacked ↑/↓ speed), rendered eagerly into bitmaps.
///
/// Why bitmaps: an `NSImage(size:flipped:drawingHandler:)` is re-run by AppKit on every status-bar
/// redraw (highlight, appearance change, screen change …), and each run went through
/// NSString drawing → CoreText attribute copying. On macOS 27.2 that path aborted twice with
/// "attempt to insert nil object" inside `CTLineCreateWithAttributedString`. Here the image is drawn
/// once per visible change, on the main thread, with no attributed strings at all: glyphs come
/// straight from cached CTFonts and are drawn with `CTFontDrawGlyphs`. AppKit only ever blits the
/// finished pixels.
enum PurrStatusImage {
    static let height: CGFloat = 22
    static let logoWidth: CGFloat = 22
    static let speedWidth: CGFloat = 70

    /// Scales baked into every image, so it stays sharp on both standard and Retina menu bars
    /// without tracking which screen the status item is on.
    private static let scales: [CGFloat] = [1, 2]

    private static let textX: CGFloat = 24.0
    private static let upTop: CGFloat = 1.0
    private static let dnTop: CGFloat = 10.8

    private struct Face {
        let font: CTFont
        /// Distance from the line's top edge to the baseline, as NSString drawing placed it.
        let baseline: CGFloat
        let alpha: CGFloat
        /// Glyph + advance per character, resolved once; characters the font lacks fall back to
        /// Menlo and then to a blank of the digit width, never to nil.
        private var cache: [UnicodeScalar: (CTFont, CGGlyph, CGFloat)] = [:]

        init(font: CTFont, baseline: CGFloat, alpha: CGFloat) {
            self.font = font
            self.baseline = baseline
            self.alpha = alpha
        }

        mutating func glyph(_ s: UnicodeScalar) -> (CTFont, CGGlyph, CGFloat) {
            if let hit = cache[s] { return hit }
            let resolved = Face.lookup(s, in: font)
                ?? Face.lookup(s, in: Face.menlo(size: CTFontGetSize(font)))
                ?? (font, 0, Face.advance(of: "0", in: font))
            cache[s] = resolved
            return resolved
        }

        private static func menlo(size: CGFloat) -> CTFont {
            CTFontCreateWithName("Menlo-Regular" as CFString, size, nil)
        }

        private static func lookup(_ s: UnicodeScalar, in font: CTFont) -> (CTFont, CGGlyph, CGFloat)? {
            var units = Array(String(Character(s)).utf16)
            var glyphs = [CGGlyph](repeating: 0, count: units.count)
            guard units.count == 1,
                  CTFontGetGlyphsForCharacters(font, &units, &glyphs, 1),
                  glyphs[0] != 0 else { return nil }
            var adv = CGSize.zero
            CTFontGetAdvancesForGlyphs(font, .horizontal, &glyphs, &adv, 1)
            return (font, glyphs[0], adv.width)
        }

        static func advance(of s: String, in font: CTFont) -> CGFloat {
            guard let scalar = s.unicodeScalars.first, let hit = lookup(scalar, in: font) else { return 5 }
            return hit.2
        }
    }

    private static var upFace = makeFace(weight: .medium, alpha: 0.82)
    private static var dnFace = makeFace(weight: .bold, alpha: 1.0)

    private static func makeFace(weight: NSFont.Weight, alpha: CGFloat) -> Face {
        let ns = NSFont.monospacedSystemFont(ofSize: 8.5, weight: weight)
        // Matches where NSString.draw(at:) put the baseline in the old drawing handler
        // (half a point above `top + ascender`; checked pixel-for-pixel at 1x and 2x).
        return Face(font: ns as CTFont, baseline: ns.ascender - 0.5, alpha: alpha)
    }

    /// Builds the status image. `upText` / `dnText` are already formatted (e.g. "↑ 1.2M").
    /// Call on the main thread.
    static func make(isAwake: Bool, hasError: Bool, speed: (up: String, dn: String)?) -> NSImage {
        let width = speed == nil ? logoWidth : speedWidth
        let size = NSSize(width: width, height: height)
        let image = NSImage(size: size)
        for scale in scales {
            if let rep = render(size: size, scale: scale, isAwake: isAwake, hasError: hasError, speed: speed) {
                image.addRepresentation(rep)
            }
        }
        image.isTemplate = true
        return image
    }

    private static func render(size: NSSize, scale: CGFloat, isAwake: Bool, hasError: Bool,
                               speed: (up: String, dn: String)?) -> NSBitmapImageRep? {
        let pw = Int((size.width * scale).rounded())
        let ph = Int((size.height * scale).rounded())
        guard pw > 0, ph > 0,
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let ctx = CGContext(data: nil, width: pw, height: ph, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: space,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        ctx.scaleBy(x: scale, y: scale)
        // Same top-left, y-down coordinate space the logo paths were authored in.
        ctx.translateBy(x: 0, y: size.height)
        ctx.scaleBy(x: 1, y: -1)

        let ns = NSGraphicsContext(cgContext: ctx, flipped: true)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = ns
        drawLogo(isAwake: isAwake, hasError: hasError)
        NSGraphicsContext.restoreGraphicsState()

        if let speed = speed {
            drawText(speed.up, face: &upFace, top: upTop, in: ctx)
            drawText(speed.dn, face: &dnFace, top: dnTop, in: ctx)
        }

        guard let cg = ctx.makeImage() else { return nil }
        let rep = NSBitmapImageRep(cgImage: cg)
        rep.size = size
        return rep
    }

    private static func drawText(_ text: String, face: inout Face, top: CGFloat, in ctx: CGContext) {
        ctx.saveGState()
        // Glyphs draw y-up: put the baseline at `top + ascender` (y-down) and flip back.
        ctx.translateBy(x: textX, y: top + face.baseline)
        ctx.scaleBy(x: 1, y: -1)
        ctx.setFillColor(CGColor(gray: 0, alpha: face.alpha))
        var x: CGFloat = 0
        for scalar in text.unicodeScalars {
            let (font, glyph, adv) = face.glyph(scalar)
            if glyph != 0 {
                var g = glyph
                var p = CGPoint(x: x, y: 0)
                CTFontDrawGlyphs(font, &g, &p, 1, ctx)
            }
            x += adv
        }
        ctx.restoreGState()
    }

    private static func drawLogo(isAwake: Bool, hasError: Bool) {
        NSColor.black.withAlphaComponent(isAwake ? 1.0 : 0.55).setStroke()

        // Cat ears outline with smooth rounded crown between ears
        let ears = NSBezierPath()
        ears.lineWidth = 1.9
        ears.lineCapStyle = .round
        ears.lineJoinStyle = .round
        ears.move(to: NSPoint(x: 1.5, y: 15.5))
        ears.line(to: NSPoint(x: 4.73, y: 6.76))
        ears.line(to: NSPoint(x: 8.15, y: 11.89))
        ears.curve(
            to: NSPoint(x: 13.85, y: 11.89),
            controlPoint1: NSPoint(x: 9.8, y: 10.9),
            controlPoint2: NSPoint(x: 12.2, y: 10.9)
        )
        ears.line(to: NSPoint(x: 17.27, y: 6.76))
        ears.line(to: NSPoint(x: 20.50, y: 15.50))
        ears.stroke()

        // Connected wave line underneath: M3 19.6 q2-1.4 4 0 t4 0 4 0 4 0
        if isAwake {
            let wave = NSBezierPath()
            wave.lineWidth = 1.8
            wave.lineCapStyle = .round
            wave.lineJoinStyle = .round
            wave.move(to: NSPoint(x: 3.0, y: 19.2))
            let pts: [(CGFloat, CGFloat, CGFloat, CGFloat)] = [
                (5.0, 17.8, 7.0, 19.2),
                (9.0, 20.6, 11.0, 19.2),
                (13.0, 17.8, 15.0, 19.2),
                (17.0, 20.6, 19.0, 19.2)
            ]
            for seg in pts {
                wave.curve(
                    to: NSPoint(x: seg.2, y: seg.3),
                    controlPoint1: NSPoint(x: seg.0, y: seg.1),
                    controlPoint2: NSPoint(x: seg.0, y: seg.1)
                )
            }
            wave.stroke()
        }

        // Error dot: <circle class="bd" cx="19.6" cy="4.2" r="2.6"/>
        if hasError {
            NSColor.black.setFill()
            let dot = NSBezierPath(ovalIn: NSRect(x: 19.6 - 2.4, y: 4.2 - 2.4, width: 4.8, height: 4.8))
            dot.fill()
        }
    }
}
