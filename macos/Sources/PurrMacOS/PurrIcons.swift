import SwiftUI
import AppKit
import Combine

// MARK: - Self-driven animation
/// Base for views that animate from their own timer, with no SwiftUI work per frame: a TimelineView
/// tick, even in a small subtree, re-ran layout of the whole window (~15 % CPU at 15 fps).
/// `interval` is asked on every frame: a number keeps it running at that cadence, nil stops after
/// showing the current state. Frames also stop while the main window is not on screen.
class PurrTickingView: NSView {
    var interval: (() -> TimeInterval?)?
    private var timer: Timer?
    private var currentInterval: TimeInterval = 0
    private var visibilityObserver: AnyCancellable?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        visibilityObserver = PurrVisibility.shared.$mainWindowVisible.sink { [weak self] _ in
            DispatchQueue.main.async { self?.kick() }
        }
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    deinit { timer?.invalidate() }

    override var isOpaque: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        kick()
    }

    /// Shows the state for time `t` (reference-date seconds).
    func render(at t: Double) {}

    private func stop() {
        timer?.invalidate()
        timer = nil
        currentInterval = 0
    }

    /// Re-evaluates whether frames are needed (called on SwiftUI updates and visibility changes).
    func kick() {
        render(at: PurrClock.ref(Date()))
        guard window != nil, PurrVisibility.shared.mainWindowVisible, let iv = interval?() else {
            stop()
            return
        }
        guard iv != currentInterval || timer == nil else { return }
        stop()
        currentInterval = iv
        // Fire on a global grid so every ticking view with the same cadence updates in the same
        // display cycle (two unaligned 15 fps views would mean 30 commits per second).
        let ref = Date().timeIntervalSinceReferenceDate
        let first = Date(timeIntervalSinceReferenceDate: (ref / iv).rounded(.up) * iv)
        let t = Timer(fire: first, interval: iv, repeats: true) { [weak self] _ in self?.step() }
        t.tolerance = iv * 0.1
        RunLoop.main.add(t, forMode: .common)
        timer = t
    }

    private func step() {
        render(at: PurrClock.ref(Date()))
        guard PurrVisibility.shared.mainWindowVisible, let iv = interval?() else {
            stop()
            return
        }
        if iv != currentInterval { kick() }
    }
}

/// Line art redrawn into its own backing store each frame (ear band, hovered icon).
final class PurrDrivenView: PurrTickingView {
    var drawBlock: ((inout CGCanvasContext, CGSize, Double) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        layerContentsRedrawPolicy = .onSetNeedsDisplay
        layer?.contentsFormat = .RGBA8Uint
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override var isFlipped: Bool { true }

    // 8-bit backing store: on a wide-colour display AppKit otherwise picks half-float RGBA
    // (8 bytes per pixel) for flat sRGB line art. AppKit re-picks it when the view joins a window.
    override func viewDidMoveToWindow() {
        layer?.contentsFormat = .RGBA8Uint
        super.viewDidMoveToWindow()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        layer?.contentsFormat = .RGBA8Uint
    }

    override func render(at t: Double) { needsDisplay = true }

    override func draw(_ dirtyRect: NSRect) {
        guard let block = drawBlock, let cg = NSGraphicsContext.current?.cgContext else { return }
        var ctx = CGCanvasContext(cg: cg)
        block(&ctx, bounds.size, PurrClock.ref(Date()))
    }
}

struct PurrDrivenCanvas: NSViewRepresentable {
    let size: CGSize
    let interval: () -> TimeInterval?
    let draw: (inout CGCanvasContext, CGSize, Double) -> Void

    func makeNSView(context: Context) -> PurrDrivenView {
        let v = PurrDrivenView(frame: CGRect(origin: .zero, size: size))
        v.drawBlock = draw
        v.interval = interval
        return v
    }

    func updateNSView(_ nsView: PurrDrivenView, context: Context) {
        nsView.drawBlock = draw
        nsView.interval = interval
        nsView.kick()
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: PurrDrivenView, context: Context) -> CGSize? {
        size
    }
}

/// One round-capped, round-joined stroke of the ear band.
struct PurrStroke {
    let path: CGPath
    let color: NSColor
    let width: CGFloat
    let dash: [CGFloat]

    func stroke(in cg: CGContext) {
        cg.saveGState()
        cg.setStrokeColor(color.cgColor)
        cg.setLineWidth(width)
        cg.setLineCap(.round)
        cg.setLineJoin(.round)
        if !dash.isEmpty { cg.setLineDash(phase: 0, lengths: dash) }
        cg.addPath(path)
        cg.strokePath()
        cg.restoreGState()
    }
}

/// Strokes shown through CAShapeLayers: a frame hands Core Animation new paths instead of
/// rasterizing a large backing store in-process (which also made Core Animation keep a ~30 MB pool
/// of GPU surfaces for the redrawn layer).
final class PurrStrokeView: PurrTickingView {
    var strokes: ((CGSize, Double) -> [PurrStroke])?
    private let container = CALayer()
    private var shapes: [CAShapeLayer] = []

    override init(frame: NSRect) {
        super.init(frame: frame)
        container.isGeometryFlipped = true
        container.actions = ["bounds": NSNull(), "position": NSNull()]
        layer?.addSublayer(container)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        container.frame = bounds
        shapes.forEach { $0.frame = bounds }
        CATransaction.commit()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 2
        shapes.forEach { $0.contentsScale = scale }
    }

    override func render(at t: Double) {
        guard let strokes = strokes else { return }
        var list: [PurrStroke] = []
        effectiveAppearance.performAsCurrentDrawingAppearance {
            list = strokes(bounds.size, t)
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let scale = window?.backingScaleFactor ?? 2
        while shapes.count < list.count {
            let l = CAShapeLayer()
            l.fillColor = nil
            l.lineCap = .round
            l.lineJoin = .round
            l.contentsScale = scale
            l.frame = bounds
            container.addSublayer(l)
            shapes.append(l)
        }
        for (i, l) in shapes.enumerated() {
            guard i < list.count else {
                l.isHidden = true
                continue
            }
            let st = list[i]
            l.isHidden = false
            l.path = st.path
            l.strokeColor = st.color.cgColor
            l.lineWidth = st.width
            l.lineDashPattern = st.dash.isEmpty ? nil : st.dash.map { NSNumber(value: Double($0)) }
        }
        CATransaction.commit()
    }
}

struct PurrStrokeCanvas: NSViewRepresentable {
    let size: CGSize
    let interval: () -> TimeInterval?
    let strokes: (CGSize, Double) -> [PurrStroke]

    func makeNSView(context: Context) -> PurrStrokeView {
        let v = PurrStrokeView(frame: CGRect(origin: .zero, size: size))
        v.strokes = strokes
        v.interval = interval
        return v
    }

    func updateNSView(_ nsView: PurrStrokeView, context: Context) {
        nsView.strokes = strokes
        nsView.interval = interval
        nsView.kick()
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: PurrStrokeView, context: Context) -> CGSize? {
        size
    }
}

/// Big hero glyphs (PRRRR / zzz) laid out like SwiftUI's `HStack(alignment: .bottom)` of
/// `Text(.system(size:, weight: .black))`. Each glyph is rendered once into its own layer; a frame
/// only moves those layers. Redrawing a big glyph area 15 times a second made Core Animation keep a
/// ~30 MB pool of GPU surfaces; moving static layers needs none.
struct PurrGlyphRow: Equatable {
    let chars: [String]
    let fontSizes: [CGFloat]
    let colors: [NSColor]
    let spacing: CGFloat
    let sizes: [CGSize]
    let size: CGSize

    init(chars: [String], fontSizes: [CGFloat], colors: [NSColor], spacing: CGFloat) {
        self.chars = chars
        self.fontSizes = fontSizes
        self.colors = colors
        self.spacing = spacing
        var sz: [CGSize] = []
        for i in chars.indices {
            // Exact advance widths, as SwiftUI lays out Text; only the bitmaps are rounded up.
            let s = Self.attributed(chars[i], fontSizes[min(i, fontSizes.count - 1)], colors[min(i, colors.count - 1)]).size()
            sz.append(CGSize(width: s.width, height: ceil(s.height)))
        }
        sizes = sz
        let w = sz.reduce(0) { $0 + $1.width } + spacing * CGFloat(max(0, sz.count - 1))
        size = CGSize(width: ceil(w), height: sz.map(\.height).max() ?? 0)
    }

    static func attributed(_ c: String, _ fontSize: CGFloat, _ color: NSColor) -> NSAttributedString {
        NSAttributedString(string: c, attributes: [
            .font: NSFont.systemFont(ofSize: fontSize, weight: .black),
            .foregroundColor: color
        ])
    }

    func image(_ i: Int, scale: CGFloat) -> CGImage? {
        let s = sizes[i]
        let pw = Int((s.width * scale).rounded(.up)), ph = Int((s.height * scale).rounded(.up))
        guard pw > 0, ph > 0, let cg = CGContext(
            data: nil, width: pw, height: ph, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else { return nil }
        cg.scaleBy(x: scale, y: scale)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: false)
        Self.attributed(chars[i], fontSizes[min(i, fontSizes.count - 1)], colors[min(i, colors.count - 1)]).draw(at: .zero)
        NSGraphicsContext.restoreGraphicsState()
        return cg.makeImage()
    }
}

final class PurrGlyphView: PurrTickingView {
    var row: PurrGlyphRow?
    var margin: CGFloat = 0
    /// Vertical offset of glyph i at time t (SwiftUI `.offset(y:)`: positive moves down).
    var offsets: ((Int, Double) -> CGFloat)?
    private var built: PurrGlyphRow?
    private var builtScale: CGFloat = 0
    private var glyphLayers: [CALayer] = []

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        built = nil
        render(at: PurrClock.ref(Date()))
    }

    private func rebuildIfNeeded() {
        guard let row = row, let host = layer else { return }
        let scale = window?.backingScaleFactor ?? 2
        if built == row && builtScale == scale { return }
        glyphLayers.forEach { $0.removeFromSuperlayer() }
        glyphLayers = row.chars.indices.map { i in
            let l = CALayer()
            l.contents = row.image(i, scale: scale)
            l.contentsScale = scale
            l.anchorPoint = .zero
            let sz = row.sizes[i]
            l.bounds = CGRect(x: 0, y: 0, width: ceil(sz.width), height: sz.height)
            l.actions = ["position": NSNull(), "bounds": NSNull(), "contents": NSNull()]
            host.addSublayer(l)
            return l
        }
        built = row
        builtScale = scale
    }

    override func render(at t: Double) {
        rebuildIfNeeded()
        guard let row = row else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        var x = margin
        for (i, l) in glyphLayers.enumerated() where i < row.sizes.count {
            let dy = offsets?(i, t) ?? 0
            // Unflipped layer space: bottoms aligned on the margin line; +dy (down) lowers y.
            l.position = CGPoint(x: x, y: margin - dy)
            x += row.sizes[i].width + row.spacing
        }
        CATransaction.commit()
    }
}

struct PurrGlyphCanvas: NSViewRepresentable {
    let row: PurrGlyphRow
    let margin: CGFloat
    let interval: () -> TimeInterval?
    let offsets: (Int, Double) -> CGFloat

    var size: CGSize { CGSize(width: row.size.width + margin * 2, height: row.size.height + margin * 2) }

    func makeNSView(context: Context) -> PurrGlyphView {
        let v = PurrGlyphView(frame: CGRect(origin: .zero, size: size))
        updateNSView(v, context: context)
        return v
    }

    func updateNSView(_ nsView: PurrGlyphView, context: Context) {
        nsView.row = row
        nsView.margin = margin
        nsView.offsets = offsets
        nsView.interval = interval
        nsView.kick()
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: PurrGlyphView, context: Context) -> CGSize? {
        size
    }
}

// MARK: - Bounded animation schedules
/// Frames every `interval` seconds until `until` (nil = no end), then nothing more.
/// `.animation(paused:)` decides "paused" only when the body is evaluated, so a hover animation
/// that should stop 0.65 s later kept ticking at 60 fps until something else re-rendered it.
/// A finite schedule ends on its own.
struct PurrFrames: TimelineSchedule {
    var interval: TimeInterval
    var until: Date? = nil
    /// After `until`: keep going at this slower interval (ambient motion) instead of stopping.
    var thenInterval: TimeInterval? = nil
    var paused: Bool = false

    func entries(from startDate: Date, mode: TimelineScheduleMode) -> AnyIterator<Date> {
        if paused {
            var done = false
            return AnyIterator {
                if done { return nil }
                done = true
                return startDate
            }
        }
        // Snap to a global grid so every view on the same cadence ticks on the same instant and
        // SwiftUI commits them in one display cycle (two unaligned 15 fps views = 30 commits/s).
        func snap(_ d: Date, _ i: TimeInterval) -> Date {
            let r = d.timeIntervalSinceReferenceDate
            return Date(timeIntervalSinceReferenceDate: (r / i).rounded(.up) * i)
        }
        var t = startDate
        let end = until
        let step = interval
        let slow = thenInterval
        var first = true
        return AnyIterator {
            if first {
                // Always render the current state immediately, then continue on the grid.
                first = false
                let now = t
                t = snap(now.addingTimeInterval(0.001), step)
                return now
            }
            if let end = end, t > end {
                guard let slow = slow else { return nil }
                let n = snap(t, slow)
                t = n.addingTimeInterval(slow)
                return n
            }
            defer { t += step }
            return t
        }
    }
}

/// Cat-switch schedule: only the instants where the blink starts / ends (phase 0.94 / 0.99 of a
/// 4.2 s cycle on the reference clock), plus 60 fps frames while a hover twitch is playing.
struct PurrBlinkSchedule: TimelineSchedule {
    var blinking: Bool
    var twitchUntil: Date

    func entries(from startDate: Date, mode: TimelineScheduleMode) -> AnyIterator<Date> {
        let cycle = 4.2
        let edges = [0.94 * cycle, 0.99 * cycle]
        var t = startDate
        var first = true
        let blinking = self.blinking
        let twitchUntil = self.twitchUntil
        return AnyIterator {
            if first {
                first = false
                return t
            }
            var next: Date? = nil
            if t < twitchUntil {
                next = min(t.addingTimeInterval(1.0 / 60.0), twitchUntil.addingTimeInterval(0.001))
            }
            if blinking {
                let ref = t.timeIntervalSinceReferenceDate
                let base = floor(ref / cycle) * cycle
                var edge = Double.infinity
                for k in 0..<2 {
                    for e in edges {
                        let c = base + Double(k) * cycle + e + 0.001
                        if c > ref + 0.0005 { edge = min(edge, c) }
                    }
                }
                let ed = Date(timeIntervalSinceReferenceDate: edge)
                next = next.map { min($0, ed) } ?? ed
            }
            guard let n = next else { return nil }
            t = n
            return n
        }
    }
}

// MARK: - Zero-IOSurface CoreGraphics Canvas Wrapper (eliminates per-Canvas CAMetalLayer / IOSurface overhead)
struct CGCanvasContext {
    let cg: CGContext
    var opacity: Double = 1.0

    enum Shading {
        case color(Color)
    }

    func clip(to path: Path) {
        cg.addPath(path.cgPath)
        cg.clip()
    }

    func stroke(_ path: Path, with shading: Shading, style: StrokeStyle) {
        cg.saveGState()
        if case .color(let c) = shading {
            let nsc = NSColor(c)
            nsc.withAlphaComponent(nsc.alphaComponent * CGFloat(opacity)).setStroke()
        }
        cg.setLineWidth(style.lineWidth)
        cg.setLineCap(style.lineCap)
        cg.setLineJoin(style.lineJoin)
        if !style.dash.isEmpty {
            cg.setLineDash(phase: style.dashPhase, lengths: style.dash)
        }
        cg.addPath(path.cgPath)
        cg.strokePath()
        cg.restoreGState()
    }

    func stroke(_ path: Path, with shading: Shading, lineWidth: CGFloat) {
        stroke(path, with: shading, style: StrokeStyle(lineWidth: lineWidth))
    }

    func drawText(_ text: String, color: Color, at pt: CGPoint) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 10, weight: .regular),
            .foregroundColor: NSColor(color)
        ]
        let ns = text as NSString
        let sz = ns.size(withAttributes: attrs)
        ns.draw(at: CGPoint(x: pt.x - sz.width * 0.5, y: pt.y - sz.height * 0.5), withAttributes: attrs)
    }

    func fill(_ path: Path, with shading: Shading) {
        cg.saveGState()
        if case .color(let c) = shading {
            let nsc = NSColor(c)
            nsc.withAlphaComponent(nsc.alphaComponent * CGFloat(opacity)).setFill()
        }
        cg.addPath(path.cgPath)
        cg.fillPath()
        cg.restoreGState()
    }

    func translateBy(x: CGFloat, y: CGFloat) {
        cg.translateBy(x: x, y: y)
    }

    func rotate(by angle: Angle) {
        cg.rotate(by: CGFloat(angle.radians))
    }

    func scaleBy(x: CGFloat, y: CGFloat) {
        cg.scaleBy(x: x, y: y)
    }

    func drawLayer(_ body: (inout CGCanvasContext) -> Void) {
        cg.saveGState()
        var sub = self
        body(&sub)
        cg.restoreGState()
    }
}

struct PurrCanvasView: View {
    let size: CGSize
    let draw: (inout CGCanvasContext, CGSize) -> Void

    var body: some View {
        let img = NSImage(size: size, flipped: true) { _ in
            guard let cg = NSGraphicsContext.current?.cgContext else { return false }
            var ctx = CGCanvasContext(cg: cg)
            draw(&ctx, size)
            return true
        }
        Image(nsImage: img)
            .frame(width: size.width, height: size.height)
    }
}

// MARK: - Brand Mark (Scalable crisp vector from PURR-desktop-v5.html with rounded crown)
struct PurrMarkView: View {
    let isAwake: Bool
    let bg: Color
    let ink: Color
    var size: CGFloat = 22

    var body: some View {
        PurrCanvasView(size: CGSize(width: size, height: size)) { ctx, sz in
            let s = sz.width / 22.0
            let clipRect = CGRect(x: 1 * s, y: 1 * s, width: 20 * s, height: 20 * s)
            let clipPath = Path(roundedRect: clipRect, cornerRadius: 5 * s)
            ctx.clip(to: clipPath)
            ctx.fill(Path(CGRect(origin: .zero, size: sz)), with: .color(bg))

            var sil = Path()
            sil.move(to: CGPoint(x: 0 * s, y: 14.0 * s))
            sil.addLine(to: CGPoint(x: 1.0 * s, y: 14.0 * s))
            sil.addLine(to: CGPoint(x: 4.4 * s, y: 4.8 * s))
            sil.addLine(to: CGPoint(x: 8.0 * s, y: 10.2 * s))
            // Smooth rounded crown between the ears instead of a sharp point
            sil.addQuadCurve(to: CGPoint(x: 14.0 * s, y: 10.2 * s), control: CGPoint(x: 11.0 * s, y: 8.8 * s))
            sil.addLine(to: CGPoint(x: 17.6 * s, y: 4.8 * s))
            sil.addLine(to: CGPoint(x: 21.0 * s, y: 14.0 * s))
            sil.addLine(to: CGPoint(x: 22.0 * s, y: 14.0 * s))
            sil.addLine(to: CGPoint(x: 22.0 * s, y: 22.0 * s))
            sil.addLine(to: CGPoint(x: 0 * s, y: 22.0 * s))
            sil.closeSubpath()
            ctx.fill(sil, with: .color(ink))

            var eyes = Path()
            if isAwake {
                eyes.move(to: CGPoint(x: 7.1 * s, y: 16.8 * s))
                eyes.addQuadCurve(to: CGPoint(x: 9.7 * s, y: 16.8 * s), control: CGPoint(x: 8.4 * s, y: 18.2 * s))
                eyes.move(to: CGPoint(x: 12.3 * s, y: 16.8 * s))
                eyes.addQuadCurve(to: CGPoint(x: 14.9 * s, y: 16.8 * s), control: CGPoint(x: 13.6 * s, y: 18.2 * s))
            } else {
                eyes.move(to: CGPoint(x: 7.3 * s, y: 17.2 * s))
                eyes.addLine(to: CGPoint(x: 9.7 * s, y: 17.2 * s))
                eyes.move(to: CGPoint(x: 12.3 * s, y: 17.2 * s))
                eyes.addLine(to: CGPoint(x: 14.7 * s, y: 17.2 * s))
            }
            ctx.stroke(eyes, with: .color(bg), style: StrokeStyle(lineWidth: 1.6 * s, lineCap: .round))
        }
        .frame(width: size, height: size)
    }
}

// MARK: - Twitch Curve Helper (30% { rotate(-8deg) translateY(-0.6px) } 60% { rotate(5deg) } 100% { 0 })
func evaluateTwitch(elapsed: Double, duration: Double = 0.45) -> (deg: Double, dy: CGFloat) {
    guard elapsed >= 0 && elapsed < duration else { return (0, 0) }
    let p = elapsed / duration
    if p < 0.30 {
        let u = p / 0.30
        let s = sin(u * .pi * 0.5)
        return (-8.0 * s, CGFloat(-0.6 * s))
    } else if p < 0.65 {
        let u = (p - 0.30) / 0.35
        let c = (1.0 - cos(u * .pi)) * 0.5
        let deg = -8.0 + 13.0 * c // -8 -> +5
        let dy = -0.6 * (1.0 - c)
        return (deg, CGFloat(dy))
    } else {
        let u = (p - 0.65) / 0.35
        let c = (1.0 + cos(u * .pi)) * 0.5
        return (5.0 * c, 0)
    }
}

// MARK: - Cat Head Switch (.catsw in PURR-desktop-v5.html)
struct CatSwitchView: View {
    let isOn: Bool
    let ink: Color
    let bg: Color
    let mute: Color
    let line: Color
    var externalHover: Bool = false

    @State private var localHover: Bool = false
    @State private var twitchStart: TimeInterval = -10.0
    @State private var twitchUntil: Date = .distantPast

    private var isHovered: Bool { localHover || externalHover }

    private func startTwitch() {
        twitchStart = PurrClock.now()
        twitchUntil = Date().addingTimeInterval(0.5)
    }

    var body: some View {
        // Blink edges only (2 frames per 4.2 s) plus 60 fps during the 0.45 s hover twitch.
        TimelineView(PurrBlinkSchedule(blinking: isOn, twitchUntil: twitchUntil)) { tl in
            let now = PurrClock.now()
            let (deg, dy) = evaluateTwitch(elapsed: now - twitchStart, duration: 0.45)
            let t = PurrClock.ref(tl.date)
            let phase = t.truncatingRemainder(dividingBy: 4.2) / 4.2
            let blinkScale: CGFloat = (isOn && phase > 0.94 && phase < 0.99) ? 0.12 : 1.0

            ZStack(alignment: .leading) {
                // Striped or filled track
                PurrCanvasView(size: CGSize(width: 56, height: 30)) { ctx, size in
                    let rect = CGRect(origin: .zero, size: size)
                    let clip = Path(roundedRect: rect, cornerRadius: 15)
                    ctx.clip(to: clip)
                    if isOn {
                        ctx.fill(clip, with: .color(ink))
                    } else {
                        ctx.fill(clip, with: .color(bg))
                        var x: CGFloat = 0
                        while x < size.width {
                            let stripe = CGRect(x: x, y: 0, width: 1.0, height: size.height)
                            ctx.fill(Path(stripe), with: .color(line))
                            x += 5.0
                        }
                    }
                }
                .overlay(
                    RoundedRectangle(cornerRadius: 15)
                        .stroke(ink, lineWidth: 1.5)
                )

                if !isOn {
                    Text("z")
                        .font(.system(size: 11, weight: .heavy, design: .monospaced))
                        .foregroundColor(mute)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .padding(.trailing, 9)
                        .offset(y: -1)
                }

                // Cat head knob with cubic-bezier(.35, 1.7, .5, 1) slide + left-right twitch on hover
                CatHeadKnobShapeView(isOn: isOn, blinkScale: blinkScale, ink: ink, bg: bg)
                    .frame(width: 28, height: 28)
                    .rotationEffect(.degrees(deg), anchor: .center)
                    .offset(x: isOn ? 24 : 0, y: -1 + dy)
                    .animation(.timingCurve(0.35, 1.7, 0.5, 1.0, duration: 0.5), value: isOn)
            }
        }
        .frame(width: 56, height: 30)
        .onHover { inside in
            localHover = inside
            if inside {
                startTwitch()
            }
        }
        .onChange(of: externalHover) { _, newValue in
            if newValue {
                startTwitch()
            }
        }
    }
}

struct CatHeadKnobShapeView: View {
    let isOn: Bool
    let blinkScale: CGFloat
    let ink: Color
    let bg: Color

    var body: some View {
        PurrCanvasView(size: CGSize(width: 28, height: 28)) { ctx, size in
            let s = size.width / 24.0
            let headPath = Self.catHeadPath(scale: s)
            ctx.fill(headPath, with: .color(bg))
            ctx.stroke(
                headPath,
                with: .color(isOn ? bg : ink),
                style: StrokeStyle(lineWidth: 1.75 * s, lineCap: .round, lineJoin: .round)
            )
            if !isOn {
                var sleepEyes = Path()
                sleepEyes.move(to: CGPoint(x: 7.6 * s, y: 13.4 * s))
                sleepEyes.addQuadCurve(to: CGPoint(x: 10.0 * s, y: 13.4 * s), control: CGPoint(x: 8.8 * s, y: 14.7 * s))
                sleepEyes.move(to: CGPoint(x: 14.0 * s, y: 13.4 * s))
                sleepEyes.addQuadCurve(to: CGPoint(x: 16.4 * s, y: 13.4 * s), control: CGPoint(x: 15.2 * s, y: 14.7 * s))
                ctx.stroke(sleepEyes, with: .color(ink), style: StrokeStyle(lineWidth: 1.75 * s, lineCap: .round))
            } else {
                let r = 1.3 * s
                let ry = r * blinkScale
                let e1 = CGRect(x: 8.8 * s - r, y: 13.4 * s - ry, width: r * 2, height: ry * 2)
                let e2 = CGRect(x: 15.2 * s - r, y: 13.4 * s - ry, width: r * 2, height: ry * 2)
                ctx.fill(Path(ellipseIn: e1), with: .color(ink))
                ctx.fill(Path(ellipseIn: e2), with: .color(ink))
            }
        }
    }

    static func catHeadPath(scale s: CGFloat) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: 5.0 * s, y: 9.6 * s))
        p.addLine(to: CGPoint(x: 5.4 * s, y: 3.8 * s))
        p.addLine(to: CGPoint(x: 9.5 * s, y: 7.4 * s))
        p.addQuadCurve(to: CGPoint(x: 14.5 * s, y: 7.4 * s), control: CGPoint(x: 12.0 * s, y: 5.8 * s))
        p.addLine(to: CGPoint(x: 18.6 * s, y: 3.8 * s))
        p.addLine(to: CGPoint(x: 19.0 * s, y: 9.6 * s))
        p.addCurve(
            to: CGPoint(x: 21.0 * s, y: 14.5 * s),
            control1: CGPoint(x: 20.3 * s, y: 11.1 * s),
            control2: CGPoint(x: 21.0 * s, y: 12.8 * s)
        )
        p.addCurve(
            to: CGPoint(x: 12.0 * s, y: 20.5 * s),
            control1: CGPoint(x: 21.0 * s, y: 18.5 * s),
            control2: CGPoint(x: 17.0 * s, y: 20.5 * s)
        )
        p.addCurve(
            to: CGPoint(x: 3.0 * s, y: 14.5 * s),
            control1: CGPoint(x: 7.0 * s, y: 20.5 * s),
            control2: CGPoint(x: 3.0 * s, y: 18.5 * s)
        )
        p.addCurve(
            to: CGPoint(x: 5.0 * s, y: 9.6 * s),
            control1: CGPoint(x: 3.0 * s, y: 12.8 * s),
            control2: CGPoint(x: 3.7 * s, y: 11.1 * s)
        )
        p.closeSubpath()
        return p
    }
}

// MARK: - Vector Icons (Bespoke internal element animation per icon, no whole-icon shaking)
struct PurrIconView: View {
    let name: String
    var size: CGFloat = 18
    var strokeWidth: CGFloat = 1.75
    var isAwake: Bool = false
    var color: Color = .primary
    var isHovered: Bool = false
    var allowSelfHover: Bool = true

    @State private var selfHover: Bool = false
    @State private var hoverStartedAt: TimeInterval = -10.0
    /// Wall-clock end of the current hover animation; distantFuture while the pointer is inside.
    @State private var animateUntil: Date = .distantPast

    private var activeHover: Bool { isHovered || (allowSelfHover && selfHover) }

    private func hoverChanged(_ inside: Bool) {
        if inside {
            hoverStartedAt = PurrClock.now()
            animateUntil = .distantFuture
        } else {
            // Let the settle-out wave finish (0.65 s after the hover began), then stop ticking.
            let left = max(0.0, 0.65 - (PurrClock.now() - hoverStartedAt))
            animateUntil = Date().addingTimeInterval(left + 0.05)
        }
    }

    private func drawFrame(_ ctx: inout CGCanvasContext, _ sz: CGSize, now: Double) {
        let shouldAnimate = activeHover || (now - hoverStartedAt < 0.65)
        let elapsed = now - hoverStartedAt
        let prog: Double = min(1.0, max(0.0, elapsed / 0.55))
        let wave: Double = activeHover ? sin(now * 6.5) : (elapsed < 0.65 ? sin(prog * .pi * 2.0) * (1.0 - prog) : 0.0)
        drawIcon(ctx: &ctx, sz: sz, now: now, elapsed: elapsed, prog: prog, wave: wave, shouldAnimate: shouldAnimate)
    }

    var body: some View {
        let wantsFrames = activeHover || (PurrClock.now() - hoverStartedAt < 0.65) || animateUntil > Date()
        Group {
            if wantsFrames && !PurrSnapshot.active {
                // Hover wiggle: the icon redraws itself (30 fps) in its own small layer while the pointer
                // is on it and for the 0.65 s settle-out. Through SwiftUI every one of those frames
                // re-laid-out the whole window (~15-20 % CPU for a pointer resting on an icon).
                let icon = self
                PurrDrivenCanvas(
                    size: CGSize(width: size, height: size),
                    interval: { icon.activeHover || PurrClock.now() - icon.hoverStartedAt < 0.65 ? 1.0 / 30.0 : nil },
                    draw: { ctx, sz, _ in icon.drawFrame(&ctx, sz, now: PurrClock.now()) }
                )
            } else {
                TimelineView(PurrFrames(interval: 1.0 / 60.0, until: activeHover ? .distantFuture : animateUntil, paused: !wantsFrames)) { _ in
                    let now = PurrClock.now()
                    PurrCanvasView(size: CGSize(width: size, height: size)) { ctx, sz in
                        drawFrame(&ctx, sz, now: now)
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .onHover { inside in
            guard allowSelfHover else { return }
            selfHover = inside
            hoverChanged(inside || isHovered)
        }
        .onChange(of: isHovered) { _, newVal in
            hoverChanged(newVal || (allowSelfHover && selfHover))
        }
    }

    private func drawIcon(ctx: inout CGCanvasContext, sz: CGSize, now: Double, elapsed: Double, prog: Double, wave: Double, shouldAnimate: Bool) {
        let s = sz.width / 24.0
        let st = StrokeStyle(lineWidth: strokeWidth * (sz.width / 18.0), lineCap: .round, lineJoin: .round)

        switch name {
                case "ov":
                    // Cat head stays still; pupils smoothly glance left-right on hover (never switching to a thick stroke)
                    let hp = CatHeadKnobShapeView.catHeadPath(scale: s)
                    ctx.stroke(hp, with: .color(color), style: st)
                    let lookX = CGFloat(wave * 0.95) * s
                    if isAwake || shouldAnimate {
                        let r = 1.2 * s
                        ctx.fill(Path(ellipseIn: CGRect(x: 8.8*s + lookX - r, y: 13.4*s - r, width: r*2, height: r*2)), with: .color(color))
                        ctx.fill(Path(ellipseIn: CGRect(x: 15.2*s + lookX - r, y: 13.4*s - r, width: r*2, height: r*2)), with: .color(color))
                    } else {
                        var e = Path()
                        e.move(to: CGPoint(x: 7.7*s, y: 13.5*s))
                        e.addQuadCurve(to: CGPoint(x: 9.9*s, y: 13.5*s), control: CGPoint(x: 8.8*s, y: 14.5*s))
                        e.move(to: CGPoint(x: 14.1*s, y: 13.5*s))
                        e.addQuadCurve(to: CGPoint(x: 16.3*s, y: 13.5*s), control: CGPoint(x: 15.2*s, y: 14.5*s))
                        let thinEye = StrokeStyle(lineWidth: st.lineWidth * 0.78, lineCap: .round, lineJoin: .round)
                        ctx.stroke(e, with: .color(color), style: thinEye)
                    }
                    let ny = CGFloat(wave * 0.35) * s
                    var nose = Path()
                    nose.move(to: CGPoint(x: 11.2*s + lookX * 0.3, y: 16.2*s + ny))
                    nose.addLine(to: CGPoint(x: 12.8*s + lookX * 0.3, y: 16.2*s + ny))
                    nose.addLine(to: CGPoint(x: 12.0*s + lookX * 0.3, y: 17.0*s + ny))
                    nose.closeSubpath()
                    ctx.fill(nose, with: .color(color))

                case "px":
                    // Bowl stays still; signal waves ripple upward sequentially
                    var bowl = Path()
                    bowl.move(to: CGPoint(x: 2.8*s, y: 14.5*s))
                    bowl.addLine(to: CGPoint(x: 21.2*s, y: 14.5*s))
                    bowl.move(to: CGPoint(x: 3.6*s, y: 14.5*s))
                    bowl.addCurve(to: CGPoint(x: 12.0*s, y: 20.3*s), control1: CGPoint(x: 4.0*s, y: 18.3*s), control2: CGPoint(x: 7.4*s, y: 20.3*s))
                    bowl.addCurve(to: CGPoint(x: 20.4*s, y: 14.5*s), control1: CGPoint(x: 16.6*s, y: 20.3*s), control2: CGPoint(x: 20.0*s, y: 18.3*s))
                    ctx.stroke(bowl, with: .color(color), style: st)
                    let op1 = shouldAnimate ? (0.3 + 0.7 * abs(sin(now * 5.5))) : 1.0
                    let op2 = shouldAnimate ? (0.3 + 0.7 * abs(sin(now * 5.5 - 1.1))) : 1.0
                    var w1 = Path()
                    w1.move(to: CGPoint(x: 9.0*s, y: 10.6*s))
                    w1.addQuadCurve(to: CGPoint(x: 15.0*s, y: 10.6*s), control: CGPoint(x: 12.0*s, y: 8.0*s))
                    ctx.stroke(w1, with: .color(color.opacity(op1)), style: st)
                    var w2 = Path()
                    w2.move(to: CGPoint(x: 6.2*s, y: 7.7*s))
                    w2.addQuadCurve(to: CGPoint(x: 17.8*s, y: 7.7*s), control: CGPoint(x: 12.0*s, y: 3.5*s))
                    ctx.stroke(w2, with: .color(color.opacity(op2)), style: st)
                    ctx.fill(Path(ellipseIn: CGRect(x: 11.1*s, y: 11.7*s, width: 1.8*s, height: 1.8*s)), with: .color(color))

                case "sb":
                    // Yarn ball rotates around its own center (11,11) while loose thread waves
                    ctx.drawLayer { layer in
                        layer.translateBy(x: 11.0*s, y: 11.0*s)
                        layer.rotate(by: .degrees(wave * 24.0))
                        layer.translateBy(x: -11.0*s, y: -11.0*s)
                        let c = Path(ellipseIn: CGRect(x: (11-7.6)*s, y: (11-7.6)*s, width: 15.2*s, height: 15.2*s))
                        layer.stroke(c, with: .color(color), style: st)
                        var lines = Path()
                        lines.move(to: CGPoint(x: 4.4*s, y: 7.4*s))
                        lines.addQuadCurve(to: CGPoint(x: 14.4*s, y: 18.0*s), control: CGPoint(x: 11.0*s, y: 10.0*s))
                        lines.move(to: CGPoint(x: 8.6*s, y: 3.8*s))
                        lines.addQuadCurve(to: CGPoint(x: 18.3*s, y: 13.6*s), control: CGPoint(x: 15.0*s, y: 7.5*s))
                        lines.move(to: CGPoint(x: 3.5*s, y: 12.2*s))
                        lines.addQuadCurve(to: CGPoint(x: 11.1*s, y: 18.6*s), control: CGPoint(x: 8.5*s, y: 13.5*s))
                        layer.stroke(lines, with: .color(color), style: st)
                    }
                    var thread = Path()
                    let ty = CGFloat(wave * 1.4) * s
                    thread.move(to: CGPoint(x: 16.4*s, y: 16.4*s))
                    thread.addQuadCurve(to: CGPoint(x: 21.7*s, y: 20.0*s + ty), control: CGPoint(x: 18.5*s, y: 20.2*s - ty))
                    ctx.stroke(thread, with: .color(color), style: st)

                case "cn":
                    // Endpoints stay steady; signal flows along the S-curve
                    ctx.stroke(Path(ellipseIn: CGRect(x: 2.8*s, y: 17.2*s, width: 4*s, height: 4*s)), with: .color(color), style: st)
                    ctx.stroke(Path(ellipseIn: CGRect(x: 17.2*s, y: 2.8*s, width: 4*s, height: 4*s)), with: .color(color), style: st)
                    var fl = Path()
                    fl.move(to: CGPoint(x: 6.3*s, y: 17.7*s))
                    fl.addCurve(to: CGPoint(x: 10.6*s, y: 12.0*s), control1: CGPoint(x: 9.0*s, y: 15.0*s), control2: CGPoint(x: 7.6*s, y: 12.4*s))
                    fl.addCurve(to: CGPoint(x: 17.8*s, y: 5.2*s), control1: CGPoint(x: 13.8*s, y: 11.6*s), control2: CGPoint(x: 13.2*s, y: 8.4*s))
                    if shouldAnimate || isAwake {
                        let phase = CGFloat((now * 7.5).truncatingRemainder(dividingBy: 4.8)) * s
                        ctx.stroke(fl, with: .color(color), style: StrokeStyle(lineWidth: st.lineWidth, lineCap: .round, dash: [2.6*s, 2.2*s], dashPhase: -phase))
                    } else {
                        ctx.stroke(fl, with: .color(color), style: st)
                    }

                case "rl":
                    // Center signpost pole stays still; the two directional plates slide/turn in opposite directions
                    var pole = Path()
                    pole.move(to: CGPoint(x: 12*s, y: 21*s)); pole.addLine(to: CGPoint(x: 12*s, y: 4.2*s))
                    pole.move(to: CGPoint(x: 10.6*s, y: 4.2*s)); pole.addLine(to: CGPoint(x: 10.6*s, y: 2.4*s)); pole.addLine(to: CGPoint(x: 12*s, y: 3.4*s)); pole.addLine(to: CGPoint(x: 13.4*s, y: 2.4*s)); pole.addLine(to: CGPoint(x: 13.4*s, y: 4.2*s))
                    ctx.stroke(pole, with: .color(color), style: st)

                    let shift = CGFloat(wave * 1.4) * s
                    var signs = Path()
                    signs.move(to: CGPoint(x: 12*s, y: 6.2*s))
                    signs.addLine(to: CGPoint(x: 18.2*s + shift, y: 6.2*s))
                    signs.addLine(to: CGPoint(x: 20.2*s + shift, y: 8.2*s))
                    signs.addLine(to: CGPoint(x: 18.2*s + shift, y: 10.2*s))
                    signs.addLine(to: CGPoint(x: 12*s, y: 10.2*s))

                    signs.move(to: CGPoint(x: 12*s, y: 12.2*s))
                    signs.addLine(to: CGPoint(x: 5.8*s - shift, y: 12.2*s))
                    signs.addLine(to: CGPoint(x: 3.8*s - shift, y: 14.2*s))
                    signs.addLine(to: CGPoint(x: 5.8*s - shift, y: 16.2*s))
                    signs.addLine(to: CGPoint(x: 12*s, y: 16.2*s))
                    ctx.stroke(signs, with: .color(color), style: st)

                case "st":
                    // Slider knobs slide left & right along their horizontal tracks on hover
                    let slide = CGFloat(wave * 2.8) * s
                    let topX = 9.0*s + slide
                    let botX = 16.0*s - slide

                    var p = Path()
                    p.move(to: CGPoint(x: 3*s, y: 7*s)); p.addLine(to: CGPoint(x: max(3*s, topX - 2.4*s), y: 7*s))
                    p.move(to: CGPoint(x: min(21*s, topX + 2.4*s), y: 7*s)); p.addLine(to: CGPoint(x: 21*s, y: 7*s))
                    p.move(to: CGPoint(x: 3*s, y: 17*s)); p.addLine(to: CGPoint(x: max(3*s, botX - 3.2*s), y: 17*s))
                    p.move(to: CGPoint(x: min(21*s, botX + 2.8*s), y: 17*s)); p.addLine(to: CGPoint(x: 21*s, y: 17*s))
                    ctx.stroke(p, with: .color(color), style: st)

                    ctx.stroke(Path(ellipseIn: CGRect(x: topX - 2.4*s, y: 4.6*s, width: 4.8*s, height: 4.8*s)), with: .color(color), style: st)

                    var catKnob = Path()
                    catKnob.move(to: CGPoint(x: botX - 1.9*s, y: 14.9*s))
                    catKnob.addLine(to: CGPoint(x: botX - 1.8*s, y: 12.5*s))
                    catKnob.addLine(to: CGPoint(x: botX, y: 13.8*s))
                    catKnob.addLine(to: CGPoint(x: botX + 1.8*s, y: 12.5*s))
                    catKnob.addLine(to: CGPoint(x: botX + 1.9*s, y: 14.9*s))
                    catKnob.addArc(center: CGPoint(x: botX, y: 17.0*s), radius: 2.6*s, startAngle: .degrees(-35), endAngle: .degrees(215), clockwise: false)
                    catKnob.closeSubpath()
                    ctx.stroke(catKnob, with: .color(color), style: st)

                case "rule":
                    // Branching arrows extend outward along the fork
                    let ext = CGFloat(abs(wave) * 1.1) * s
                    var p = Path()
                    p.move(to: CGPoint(x: 12*s, y: 21*s)); p.addLine(to: CGPoint(x: 12*s, y: 13.6*s))
                    p.addLine(to: CGPoint(x: 6.4*s - ext, y: 6.4*s - ext))
                    p.move(to: CGPoint(x: 12*s, y: 13.6*s))
                    p.addLine(to: CGPoint(x: 17.6*s + ext, y: 6.4*s - ext))
                    p.move(to: CGPoint(x: 6*s - ext, y: 10.4*s - ext)); p.addLine(to: CGPoint(x: 6*s - ext, y: 6*s - ext)); p.addLine(to: CGPoint(x: 10.4*s - ext, y: 6*s - ext))
                    p.move(to: CGPoint(x: 13.6*s + ext, y: 6*s - ext)); p.addLine(to: CGPoint(x: 18*s + ext, y: 6*s - ext)); p.addLine(to: CGPoint(x: 18*s + ext, y: 10.4*s - ext))
                    ctx.stroke(p, with: .color(color), style: st)

                case "global":
                    // Globe meridian lines shift horizontally like a rotating sphere
                    ctx.stroke(Path(ellipseIn: CGRect(x: 4*s, y: 5*s, width: 16*s, height: 16*s)), with: .color(color), style: st)
                    let shift = CGFloat(wave * 2.2) * s
                    var p = Path()
                    p.move(to: CGPoint(x: 4*s, y: 13*s)); p.addLine(to: CGPoint(x: 20*s, y: 13*s))
                    p.move(to: CGPoint(x: 12*s, y: 5*s)); p.addQuadCurve(to: CGPoint(x: 12*s, y: 21*s), control: CGPoint(x: 8*s + shift, y: 13*s))
                    p.move(to: CGPoint(x: 12*s, y: 5*s)); p.addQuadCurve(to: CGPoint(x: 12*s, y: 21*s), control: CGPoint(x: 16*s + shift, y: 13*s))
                    p.move(to: CGPoint(x: 6.6*s, y: 7.4*s)); p.addLine(to: CGPoint(x: 6.3*s, y: 3*s)); p.addLine(to: CGPoint(x: 9.7*s, y: 5.4*s))
                    p.move(to: CGPoint(x: 17.4*s, y: 7.4*s)); p.addLine(to: CGPoint(x: 17.7*s, y: 3*s)); p.addLine(to: CGPoint(x: 14.3*s, y: 5.4*s))
                    ctx.stroke(p, with: .color(color), style: st)

                case "direct":
                    // Arrow glides forward horizontally
                    let dx = CGFloat(abs(wave) * 1.8) * s
                    var p = Path()
                    p.move(to: CGPoint(x: 3.5*s, y: 12*s)); p.addLine(to: CGPoint(x: 19*s + dx, y: 12*s))
                    p.move(to: CGPoint(x: 14*s + dx, y: 6.5*s)); p.addLine(to: CGPoint(x: 19.5*s + dx, y: 12*s)); p.addLine(to: CGPoint(x: 14*s + dx, y: 17.5*s))
                    p.move(to: CGPoint(x: 3.5*s, y: 8.5*s)); p.addLine(to: CGPoint(x: 3.5*s, y: 15.5*s))
                    ctx.stroke(p, with: .color(color), style: st)

                case "sys":
                    let r = Path(roundedRect: CGRect(x: 3*s, y: 6.6*s, width: 18*s, height: 11*s), cornerRadius: 2*s)
                    ctx.stroke(r, with: .color(color), style: st)
                    var p = Path()
                    p.move(to: CGPoint(x: 5.6*s, y: 6.6*s)); p.addLine(to: CGPoint(x: 7.2*s, y: 3.4*s)); p.addLine(to: CGPoint(x: 9.4*s, y: 6.6*s))
                    p.move(to: CGPoint(x: 14.6*s, y: 6.6*s)); p.addLine(to: CGPoint(x: 16.8*s, y: 3.4*s)); p.addLine(to: CGPoint(x: 18.4*s, y: 6.6*s))
                    p.move(to: CGPoint(x: 12*s, y: 17.6*s)); p.addLine(to: CGPoint(x: 12*s, y: 20.6*s))
                    p.move(to: CGPoint(x: 8.8*s, y: 20.6*s)); p.addLine(to: CGPoint(x: 15.2*s, y: 20.6*s))
                    ctx.stroke(p, with: .color(color), style: st)
                    let ex = CGFloat(wave * 0.8) * s
                    if isAwake || shouldAnimate {
                        ctx.fill(Path(ellipseIn: CGRect(x: 8.65*s + ex, y: 11.05*s, width: 1.9*s, height: 1.9*s)), with: .color(color))
                        ctx.fill(Path(ellipseIn: CGRect(x: 13.45*s + ex, y: 11.05*s, width: 1.9*s, height: 1.9*s)), with: .color(color))
                    } else {
                        var es = Path()
                        es.move(to: CGPoint(x: 8.6*s, y: 12.2*s)); es.addQuadCurve(to: CGPoint(x: 10.6*s, y: 12.2*s), control: CGPoint(x: 9.6*s, y: 13.2*s))
                        es.move(to: CGPoint(x: 13.4*s, y: 12.2*s)); es.addQuadCurve(to: CGPoint(x: 15.4*s, y: 12.2*s), control: CGPoint(x: 14.4*s, y: 13.2*s))
                        ctx.stroke(es, with: .color(color), style: st)
                    }

                case "tun":
                    var p = Path()
                    p.move(to: CGPoint(x: 2.5*s, y: 20.5*s)); p.addLine(to: CGPoint(x: 21.5*s, y: 20.5*s))
                    p.move(to: CGPoint(x: 6.2*s, y: 20.5*s)); p.addLine(to: CGPoint(x: 6.2*s, y: 13.5*s))
                    p.addArc(center: CGPoint(x: 12*s, y: 13.5*s), radius: 5.8*s, startAngle: .degrees(180), endAngle: .degrees(0), clockwise: false)
                    p.addLine(to: CGPoint(x: 17.8*s, y: 20.5*s))
                    ctx.stroke(p, with: .color(color), style: st)
                    let peekY = CGFloat(-abs(wave) * 1.1) * s
                    let peekOp = (elapsed >= 0 && elapsed < 0.65) ? abs(cos(elapsed / 0.65 * .pi)) : 1.0
                    ctx.fill(Path(ellipseIn: CGRect(x: 9.4*s, y: 14.6*s + peekY, width: 1.9*s, height: 1.9*s)), with: .color(color.opacity(peekOp)))
                    ctx.fill(Path(ellipseIn: CGRect(x: 12.6*s, y: 14.6*s + peekY, width: 1.9*s, height: 1.9*s)), with: .color(color.opacity(peekOp)))

                case "test":
                    ctx.stroke(Path(ellipseIn: CGRect(x: 4.6*s, y: 6.2*s, width: 14.8*s, height: 14.8*s)), with: .color(color), style: st)
                    var pth = Path()
                    pth.move(to: CGPoint(x: 10*s, y: 2.6*s)); pth.addLine(to: CGPoint(x: 14*s, y: 2.6*s))
                    pth.move(to: CGPoint(x: 12*s, y: 2.6*s)); pth.addLine(to: CGPoint(x: 12*s, y: 6.2*s))
                    pth.move(to: CGPoint(x: 18.6*s, y: 6.2*s)); pth.addLine(to: CGPoint(x: 20.0*s, y: 4.8*s))
                    ctx.stroke(pth, with: .color(color), style: st)
                    let ang: Double = shouldAnimate ? (-Double.pi / 4.0 + prog * Double.pi * 2.0) : (-Double.pi / 4.0)
                    let hx = CGFloat(12.0 + cos(ang) * 4.5) * s
                    let hy = CGFloat(13.6 + sin(ang) * 4.5) * s
                    var hand = Path()
                    hand.move(to: CGPoint(x: 12*s, y: 13.6*s))
                    hand.addLine(to: CGPoint(x: hx, y: hy))
                    ctx.stroke(hand, with: .color(color), style: st)

                case "imp":
                    let dy = CGFloat(abs(wave) * 1.8) * s
                    var arrow = Path()
                    arrow.move(to: CGPoint(x: 12*s, y: 3*s + dy)); arrow.addLine(to: CGPoint(x: 12*s, y: 13.6*s + dy))
                    arrow.move(to: CGPoint(x: 7.6*s, y: 9.2*s + dy)); arrow.addLine(to: CGPoint(x: 12*s, y: 13.6*s + dy)); arrow.addLine(to: CGPoint(x: 16.4*s, y: 9.2*s + dy))
                    ctx.stroke(arrow, with: .color(color), style: st)
                    var tray = Path()
                    tray.move(to: CGPoint(x: 3.8*s, y: 14.6*s))
                    tray.addCurve(to: CGPoint(x: 12*s, y: 20.4*s), control1: CGPoint(x: 4.1*s, y: 18.2*s), control2: CGPoint(x: 7.2*s, y: 20.4*s))
                    tray.addCurve(to: CGPoint(x: 20.2*s, y: 14.6*s), control1: CGPoint(x: 16.8*s, y: 20.4*s), control2: CGPoint(x: 19.9*s, y: 18.2*s))
                    ctx.stroke(tray, with: .color(color), style: st)

                case "upd":
                    ctx.drawLayer { layer in
                        layer.translateBy(x: 12*s, y: 12*s)
                        layer.rotate(by: .degrees(shouldAnimate ? prog * 360.0 : 0))
                        layer.translateBy(x: -12*s, y: -12*s)
                        var pth = Path()
                        pth.addArc(center: CGPoint(x: 12*s, y: 12*s), radius: 7.6*s, startAngle: .degrees(0), endAngle: .degrees(315), clockwise: false)
                        pth.move(to: CGPoint(x: 19.6*s, y: 3.6*s)); pth.addLine(to: CGPoint(x: 19.6*s, y: 8.2*s)); pth.addLine(to: CGPoint(x: 15.0*s, y: 8.2*s))
                        layer.stroke(pth, with: .color(color), style: st)
                    }

                case "mouse":
                    var pth = Path()
                    pth.move(to: CGPoint(x: 3*s, y: 19*s))
                    pth.addCurve(to: CGPoint(x: 10.6*s, y: 10.4*s), control1: CGPoint(x: 3.2*s, y: 14*s), control2: CGPoint(x: 6.2*s, y: 10.4*s))
                    pth.addCurve(to: CGPoint(x: 19.2*s, y: 16*s), control1: CGPoint(x: 14.4*s, y: 10.4*s), control2: CGPoint(x: 17.2*s, y: 13*s))
                    pth.addLine(to: CGPoint(x: 21.8*s, y: 19*s))
                    pth.closeSubpath()
                    ctx.stroke(pth, with: .color(color), style: st)
                    var ear = Path()
                    ear.addArc(center: CGPoint(x: 14.4*s, y: 11.0*s), radius: 2.3*s, startAngle: .degrees(190), endAngle: .degrees(20), clockwise: false)
                    ctx.stroke(ear, with: .color(color), style: st)
                    ctx.fill(Path(ellipseIn: CGRect(x: 16.6*s, y: 14.2*s, width: 2.0*s, height: 2.0*s)), with: .color(color))
                    var tail = Path()
                    let ty = CGFloat(wave * 1.4) * s
                    tail.move(to: CGPoint(x: 3*s, y: 19*s))
                    tail.addQuadCurve(to: CGPoint(x: 2.1*s, y: 15.6*s + ty), control: CGPoint(x: 0.8*s, y: 17.5*s - ty))
                    ctx.stroke(tail, with: .color(color), style: st)

                case "wake":
                    ctx.stroke(Path(ellipseIn: CGRect(x: 5*s, y: 6.6*s, width: 14*s, height: 14*s)), with: .color(color), style: st)
                    var pth = Path()
                    pth.move(to: CGPoint(x: 3.6*s, y: 7.6*s)); pth.addQuadCurve(to: CGPoint(x: 8.2*s, y: 3.2*s), control: CGPoint(x: 4.2*s, y: 3.5*s))
                    pth.move(to: CGPoint(x: 20.4*s, y: 7.6*s)); pth.addQuadCurve(to: CGPoint(x: 15.8*s, y: 3.2*s), control: CGPoint(x: 19.8*s, y: 3.5*s))
                    pth.move(to: CGPoint(x: 12*s, y: 10.2*s)); pth.addLine(to: CGPoint(x: 12*s, y: 13.8*s)); pth.addLine(to: CGPoint(x: 14.4*s, y: 15.4*s))
                    pth.move(to: CGPoint(x: 7.4*s, y: 19.6*s)); pth.addLine(to: CGPoint(x: 6.0*s, y: 21.4*s))
                    pth.move(to: CGPoint(x: 16.6*s, y: 19.6*s)); pth.addLine(to: CGPoint(x: 18.0*s, y: 21.4*s))
                    ctx.stroke(pth, with: .color(color), style: st)

                case "swap":
                    var pth = Path()
                    pth.move(to: CGPoint(x: 4*s, y: 8.5*s)); pth.addLine(to: CGPoint(x: 18.5*s, y: 8.5*s))
                    pth.move(to: CGPoint(x: 15*s, y: 5*s)); pth.addLine(to: CGPoint(x: 18.5*s, y: 8.5*s)); pth.addLine(to: CGPoint(x: 15*s, y: 12*s))
                    pth.move(to: CGPoint(x: 20*s, y: 15.5*s)); pth.addLine(to: CGPoint(x: 5.5*s, y: 15.5*s))
                    pth.move(to: CGPoint(x: 9*s, y: 12*s)); pth.addLine(to: CGPoint(x: 5.5*s, y: 15.5*s)); pth.addLine(to: CGPoint(x: 9*s, y: 19*s))
                    ctx.stroke(pth, with: .color(color), style: st)

                case "snd":
                    var pth = Path()
                    pth.move(to: CGPoint(x: 3.5*s, y: 9.4*s)); pth.addLine(to: CGPoint(x: 6.9*s, y: 9.4*s)); pth.addLine(to: CGPoint(x: 11.5*s, y: 5.5*s))
                    pth.addLine(to: CGPoint(x: 11.5*s, y: 18.5*s)); pth.addLine(to: CGPoint(x: 6.9*s, y: 14.6*s)); pth.addLine(to: CGPoint(x: 3.5*s, y: 14.6*s)); pth.closeSubpath()
                    pth.move(to: CGPoint(x: 15*s, y: 9*s)); pth.addQuadCurve(to: CGPoint(x: 15*s, y: 15*s), control: CGPoint(x: 16.5*s, y: 12*s))
                    pth.move(to: CGPoint(x: 18*s, y: 6.6*s)); pth.addQuadCurve(to: CGPoint(x: 18*s, y: 17.4*s), control: CGPoint(x: 20.8*s, y: 12*s))
                    ctx.stroke(pth, with: .color(color), style: st)

                case "port":
                    var pth = Path()
                    pth.move(to: CGPoint(x: 9*s, y: 3*s)); pth.addLine(to: CGPoint(x: 9*s, y: 7.6*s))
                    pth.move(to: CGPoint(x: 15*s, y: 3*s)); pth.addLine(to: CGPoint(x: 15*s, y: 7.6*s))
                    pth.move(to: CGPoint(x: 6.4*s, y: 7.6*s)); pth.addLine(to: CGPoint(x: 17.6*s, y: 7.6*s)); pth.addLine(to: CGPoint(x: 17.6*s, y: 10.8*s))
                    pth.addArc(center: CGPoint(x: 12*s, y: 10.8*s), radius: 5.6*s, startAngle: .degrees(0), endAngle: .degrees(180), clockwise: false)
                    pth.closeSubpath()
                    pth.move(to: CGPoint(x: 12*s, y: 16.4*s)); pth.addLine(to: CGPoint(x: 12*s, y: 21*s))
                    ctx.stroke(pth, with: .color(color), style: st)

                case "lan":
                    var pth = Path()
                    pth.move(to: CGPoint(x: 3*s, y: 11.4*s)); pth.addLine(to: CGPoint(x: 12*s, y: 4*s)); pth.addLine(to: CGPoint(x: 21*s, y: 11.4*s))
                    pth.move(to: CGPoint(x: 5.4*s, y: 9.6*s)); pth.addLine(to: CGPoint(x: 5.4*s, y: 20*s)); pth.addLine(to: CGPoint(x: 18.6*s, y: 20*s)); pth.addLine(to: CGPoint(x: 18.6*s, y: 9.6*s))
                    pth.move(to: CGPoint(x: 9.4*s, y: 15.2*s)); pth.addQuadCurve(to: CGPoint(x: 14.6*s, y: 15.2*s), control: CGPoint(x: 12*s, y: 13*s))
                    ctx.stroke(pth, with: .color(color), style: st)
                    ctx.fill(Path(ellipseIn: CGRect(x: 11.05*s, y: 16.65*s, width: 1.9*s, height: 1.9*s)), with: .color(color))

                case "v6":
                    ctx.stroke(Path(ellipseIn: CGRect(x: 3.4*s, y: 3.4*s, width: 17.2*s, height: 17.2*s)), with: .color(color), style: st)
                    ctx.stroke(Path(ellipseIn: CGRect(x: 8.9*s, y: 10.6*s, width: 6.0*s, height: 6.0*s)), with: .color(color), style: st)
                    var pth = Path()
                    pth.move(to: CGPoint(x: 14.2*s, y: 7.9*s))
                    pth.addCurve(to: CGPoint(x: 8.9*s, y: 12.6*s), control1: CGPoint(x: 12.2*s, y: 7.2*s), control2: CGPoint(x: 8.9*s, y: 9.2*s))
                    ctx.stroke(pth, with: .color(color), style: st)

                case "dns":
                    var pth = Path()
                    pth.move(to: CGPoint(x: 3.4*s, y: 12.4*s)); pth.addLine(to: CGPoint(x: 3.4*s, y: 4.4*s)); pth.addLine(to: CGPoint(x: 11.4*s, y: 4.4*s))
                    pth.addLine(to: CGPoint(x: 20.6*s, y: 13.6*s)); pth.addLine(to: CGPoint(x: 12.6*s, y: 21.6*s)); pth.closeSubpath()
                    ctx.stroke(pth, with: .color(color), style: st)
                    ctx.stroke(Path(ellipseIn: CGRect(x: 6.3*s, y: 7.3*s, width: 3.0*s, height: 3.0*s)), with: .color(color), style: st)

                case "cut":
                    let snip = CGFloat(abs(wave) * 1.4) * s
                    ctx.stroke(Path(ellipseIn: CGRect(x: 3.4*s, y: 3.8*s + snip * 0.4, width: 5.2*s, height: 5.2*s)), with: .color(color), style: st)
                    ctx.stroke(Path(ellipseIn: CGRect(x: 3.4*s, y: 15.0*s - snip * 0.4, width: 5.2*s, height: 5.2*s)), with: .color(color), style: st)
                    var pth = Path()
                    pth.move(to: CGPoint(x: 8.2*s, y: 8*s)); pth.addLine(to: CGPoint(x: 20.5*s, y: 17*s - snip))
                    pth.move(to: CGPoint(x: 8.2*s, y: 16*s)); pth.addLine(to: CGPoint(x: 20.5*s, y: 7*s + snip))
                    ctx.stroke(pth, with: .color(color), style: st)

                default:
                    let hp = CatHeadKnobShapeView.catHeadPath(scale: s)
                    ctx.stroke(hp, with: .color(color), style: st)
                }
    }
}

// MARK: - High-Density Shaded ASCII Cat Silhouette (Unmistakable Cat Shapes: Sitting / Loafing / Arched-Back)
struct DenseAsciiCatView: View {
    @ObservedObject private var vis = PurrVisibility.shared
    let kind: String // "timeout", "offline", "sub", "port"
    let ink: Color
    let bg: Color

    private static let ramp: [Character] = Array(" .,:;-=+*#%@")

    private static func distSeg(px: Double, py: Double, x1: Double, y1: Double, x2: Double, y2: Double) -> Double {
        let dx = x2 - x1
        let dy = y2 - y1
        if dx == 0 && dy == 0 { return hypot(px - x1, py - y1) }
        let t = max(0.0, min(1.0, ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)))
        return hypot(px - (x1 + t * dx), py - (y1 + t * dy))
    }

    private static func pointInPoly(x: Double, y: Double, poly: [(Double, Double)]) -> Bool {
        var inside = false
        var j = poly.count - 1
        for i in 0..<poly.count {
            let (xi, yi) = poly[i]
            let (xj, yj) = poly[j]
            if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi) {
                inside.toggle()
            }
            j = i
        }
        return inside
    }

    var body: some View {
        TimelineView(.animation(minimumInterval: 0.35, paused: !vis.mainWindowVisible)) { tl in
            let t = PurrClock.ref(tl.date)
            let tick = Int(t / 0.45)

            PurrCanvasView(size: CGSize(width: 248, height: 168)) { ctx, sz in
                let cols = 48
                let rows = 22
                let font = NSFont.monospacedSystemFont(ofSize: 6.8, weight: .bold)
                let cw: CGFloat = 4.6
                let ch: CGFloat = 7.1
                let x0 = (sz.width - CGFloat(cols) * cw) * 0.5
                let y0 = (sz.height - CGFloat(rows) * ch) * 0.5

                let alt = (tick % 2) == 1
                let tailSwing = sin(t * 2.8) * 0.06
                let baseNSC = NSColor(ink)

                let faceStamps = pureAsciiFaceStamps(kind: kind, alt: alt)
                var faceLookup: [Int: Character] = [:]
                for item in faceStamps {
                    faceLookup[item.r * cols + item.c] = item.ch
                }

                for r in 0..<rows {
                    let y = (Double(r) + 0.5) / Double(rows) * 2.0 - 1.0
                    for c in 0..<cols {
                        let pt = CGPoint(x: x0 + CGFloat(c) * cw, y: y0 + CGFloat(r) * ch)
                        if let fch = faceLookup[r * cols + c] {
                            String(fch).draw(at: pt, withAttributes: [.font: font, .foregroundColor: baseNSC])
                            continue
                        }
                        let x = (Double(c) + 0.5) / Double(cols) * 2.4 - 1.2
                        guard let cell = sampleCat(kind: kind, x: x, y: y, alt: alt, tailSwing: tailSwing) else {
                            continue
                        }
                        let color = baseNSC.withAlphaComponent(CGFloat(cell.alpha))
                        String(cell.ch).draw(at: pt, withAttributes: [.font: font, .foregroundColor: color])
                    }
                }
            }
        }
    }

    private func pureAsciiFaceStamps(kind: String, alt: Bool) -> [(r: Int, c: Int, ch: Character)] {
        var out: [(r: Int, c: Int, ch: Character)] = []
        func stamp(r0: Int, c0: Int, lines: [String]) {
            for (dr, line) in lines.enumerated() {
                for (dc, ch) in line.enumerated() {
                    if ch != " " {
                        out.append((r: r0 + dr, c: c0 + dc, ch: ch))
                    }
                }
            }
        }

        switch kind {
        case "empty":
            stamp(r0: 9, c0: 9, lines: [
                #"  ===   ===  "#,
                #"=           ="#,
                #"    _w_      "#
            ])

        case "timeout":
            // Cute squeezed "> _w_ <" dizzy/sleepy eyes formed purely from characters (\ > / and / < \)
            stamp(r0: 9, c0: 9, lines: [
                #"  \       /  "#,
                #"=  >  .  <  ="#,
                #"  /  _w_  \  "#
            ])

        case "offline":
            // Cute round curious eyes whose "@." / ".@" pupils shift left and right looking for Wi-Fi
            let eyeRow = alt ? #"= (.@) (.@) ="# : #"= (@.) (@.) ="#
            stamp(r0: 6, c0: 13, lines: [
                #"  .-.   .-.  "#,
                eyeRow,
                #"   ` _w_ `   "#
            ])

        case "sub":
            // Pitiful droopy teary eyes ("___" flat lids + "T; ;T" teary eyes + ",---," downturned mouth)
            let tearRow = alt ? #"=  T,   ,T  ="# : #"=  T;   ;T  ="#
            stamp(r0: 6, c0: 13, lines: [
                #"  ___   ___  "#,
                tearRow,
                #"    ,---,    "#
            ])

        default: // "port"
            // Fierce V-browed cat eyes ("\ /" angry brows + "(@) (@)" glaring eyes + "/^^^^^\" hiss mouth)
            stamp(r0: 9, c0: 8, lines: [
                #" \       / "#,
                #"=(@) . (@)="#,
                #"  /^^^^^\  "#
            ])
        }
        return out
    }

    private func sampleCat(kind: String, x: Double, y: Double, alt: Bool, tailSwing: Double) -> (ch: Character, alpha: Double)? {
        switch kind {
        case "empty", "timeout":
            // Loafing / lying down cat: clear round head on left with two tall pointed ears, lower loaf back on right, curled tail
            let dHead = hypot((x + 0.42) / 0.38, (y + 0.06) / 0.35)
            let earL: [(Double, Double)] = [(-0.76, -0.22), (-0.68, -0.78), (-0.44, -0.34)]
            let earR: [(Double, Double)] = [(-0.40, -0.34), (-0.16, -0.78), (-0.08, -0.22)]
            let inEar = Self.pointInPoly(x: x, y: y, poly: earL) || Self.pointInPoly(x: x, y: y, poly: earR)
            let dBody = hypot((x - 0.18) / 0.62, (y - 0.22) / 0.44)
            let inBody = dBody <= 1.0 && y <= 0.62
            let dPaws = hypot((x + 0.42) / 0.34, (y - 0.46) / 0.18)
            let inPaws = dPaws <= 1.0 && y <= 0.62
            let tPts: [(Double, Double)] = [(0.74, 0.35), (0.95, 0.48 + tailSwing), (0.85, 0.64), (0.45, 0.66)]
            var dTail = 99.0
            for i in 0..<(tPts.count - 1) {
                dTail = min(dTail, Self.distSeg(px: x, py: y, x1: tPts[i].0, y1: tPts[i].1, x2: tPts[i + 1].0, y2: tPts[i + 1].1))
            }
            let inTail = dTail < 0.085

            if kind == "timeout" {
                // Floating waiting bubbles ". o O" above the loafing cat
                if hypot(x - 0.10, y + 0.55) < 0.045 { return (".", 0.65) }
                if hypot(x - 0.28, y + 0.65) < 0.055 { return ("o", alt ? 0.85 : 0.40) }
                if hypot(x - 0.48, y + 0.75) < 0.065 { return ("O", alt ? 0.40 : 0.95) }
            }

            let inside = (dHead <= 1.0) || inEar || inBody || inPaws || inTail
            guard inside else { return nil }

            // Soft light-fur facial patch so the cute eyes & ω mouth stand out cleanly without bald holes
            let dFace = hypot((x + 0.42) / 0.28, (y + 0.02) / 0.22)
            if dFace < 1.0 {
                return (".", 0.22)
            }
            // Neck separation seam so head is clearly distinct from loaf body
            if abs(dHead - 0.96) < 0.08 && x > -0.18 && y < 0.25 {
                return nil
            }
            return shadeChar(x: x, y: y)

        case "offline", "sub":
            // Upright Sitting Cat: two pointed ears, round head, slender neck, plump pear torso, two front paws, S-curved tail
            let dHead = hypot((x + 0.20) / 0.40, (y + 0.30) / 0.34)
            let earL: [(Double, Double)]
            let earR: [(Double, Double)]
            if kind == "sub" {
                // Drooping ears for hungry expired subscription cat
                earL = [(-0.56, -0.38), (-0.66, -0.76), (-0.28, -0.56)]
                earR = [(-0.12, -0.56), (0.26, -0.76), (0.16, -0.38)]
            } else {
                earL = [(-0.54, -0.44), (-0.48, -0.92), (-0.24, -0.56)]
                earR = [(-0.16, -0.56), (0.08, -0.92), (0.14, -0.44)]
            }
            let inEar = Self.pointInPoly(x: x, y: y, poly: earL) || Self.pointInPoly(x: x, y: y, poly: earR)
            let by = (y - 0.32) / 0.56
            let bw = 0.32 + 0.14 * max(0.0, min(1.0, (y + 0.05) / 0.85))
            let dBody = hypot((x + 0.12) / bw, by)
            let dPaw1 = hypot((x + 0.34) / 0.13, (y - 0.83) / 0.10)
            let dPaw2 = hypot((x + 0.08) / 0.13, (y - 0.83) / 0.10)
            let tPts: [(Double, Double)] = [
                (0.20, 0.76), (0.48, 0.72), (0.66 + tailSwing, 0.45),
                (0.68 + tailSwing, 0.08), (0.58 - tailSwing, -0.22), (0.72 - tailSwing, -0.38)
            ]
            var dTail = 99.0
            for i in 0..<(tPts.count - 1) {
                dTail = min(dTail, Self.distSeg(px: x, py: y, x1: tPts[i].0, y1: tPts[i].1, x2: tPts[i + 1].0, y2: tPts[i + 1].1))
            }
            let inTail = dTail < 0.095

            if kind == "offline" {
                // Floating "?" marks above head
                if hypot(x + 0.72, y + 0.65) < 0.06 { return ("?", alt ? 0.95 : 0.45) }
                if hypot(x - 0.32, y + 0.68) < 0.06 { return ("?", alt ? 0.45 : 0.95) }
            } else {
                // Empty cat food bowl on the left floor next to paws
                if y > 0.68 && y < 0.88 && x > -0.95 && x < -0.52 {
                    if y < 0.74 { return ("_", 0.75) }
                    return ("=", 0.85)
                }
            }

            let inside = (dHead <= 1.0) || inEar || (dBody <= 1.0 && y > -0.15) || (dPaw1 <= 1.0) || (dPaw2 <= 1.0) || inTail
            guard inside else { return nil }

            let dFace = hypot((x + 0.20) / 0.29, (y + 0.26) / 0.21)
            if dFace < 1.0 {
                return (".", 0.22)
            }
            return shadeChar(x: x, y: y)

        default: // "port"
            // Standing Arched-Back Angry Cat with 4 legs and puffed vertical tail
            let dHead = hypot((x + 0.52) / 0.34, (y + 0.08) / 0.32)
            let earL: [(Double, Double)] = [(-0.80, -0.22), (-0.75, -0.72), (-0.55, -0.32)]
            let earR: [(Double, Double)] = [(-0.48, -0.32), (-0.28, -0.72), (-0.23, -0.22)]
            let inEar = Self.pointInPoly(x: x, y: y, poly: earL) || Self.pointInPoly(x: x, y: y, poly: earR)
            let bPts: [(Double, Double)] = [(-0.35, 0.05), (-0.12, -0.35), (0.12, -0.45), (0.35, -0.30), (0.52, 0.08)]
            var dSpine = 99.0
            for i in 0..<(bPts.count - 1) {
                dSpine = min(dSpine, Self.distSeg(px: x, py: y, x1: bPts[i].0, y1: bPts[i].1, x2: bPts[i + 1].0, y2: bPts[i + 1].1))
            }
            let inSpine = dSpine < 0.24
            let dLeg1 = Self.distSeg(px: x, py: y, x1: -0.42, y1: 0.15, x2: -0.46, y2: 0.78) < 0.085
            let dLeg2 = Self.distSeg(px: x, py: y, x1: -0.25, y1: 0.15, x2: -0.26, y2: 0.78) < 0.085
            let dLeg3 = Self.distSeg(px: x, py: y, x1: 0.35, y1: 0.15, x2: 0.34, y2: 0.78) < 0.085
            let dLeg4 = Self.distSeg(px: x, py: y, x1: 0.52, y1: 0.12, x2: 0.55, y2: 0.78) < 0.085
            let tPts: [(Double, Double)] = [(0.54, 0.02), (0.72 + tailSwing, -0.35), (0.68 - tailSwing, -0.78), (0.78, -0.88)]
            var dTail = 99.0
            for i in 0..<(tPts.count - 1) {
                dTail = min(dTail, Self.distSeg(px: x, py: y, x1: tPts[i].0, y1: tPts[i].1, x2: tPts[i + 1].0, y2: tPts[i + 1].1))
            }
            let inTail = dTail < 0.12

            if hypot(x + 0.10, y + 0.75) < 0.06 { return ("!", alt ? 1.0 : 0.5) }

            let inside = (dHead <= 1.0) || inEar || inSpine || dLeg1 || dLeg2 || dLeg3 || dLeg4 || inTail
            guard inside else { return nil }

            let dFace = hypot((x + 0.52) / 0.25, (y + 0.05) / 0.20)
            if dFace < 1.0 {
                return (".", 0.22)
            }
            return shadeChar(x: x, y: y)
        }
    }

    private func shadeChar(x: Double, y: Double) -> (ch: Character, alpha: Double) {
        let nx = (x + 0.15) / 0.75
        let ny = (y - 0.10) / 0.65
        let lum = max(0.0, min(1.0, 0.56 - 0.42 * nx - 0.36 * ny))
        let idx = max(1, min(11, Int(round((1.0 - lum * 0.78) * 11.0))))
        let alpha = 0.58 + 0.42 * (Double(idx) / 11.0)
        return (Self.ramp[idx], alpha)
    }
}

struct TroubleFaceView: View {
    let kind: String // "timeout", "offline", "sub", "port"
    let ink: Color
    let bg: Color

    var body: some View {
        DenseAsciiCatView(kind: kind, ink: ink, bg: bg)
    }
}
