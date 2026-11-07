import SwiftUI
import AppKit

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

    private var isHovered: Bool { localHover || externalHover }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: (PurrClock.now() - twitchStart) > 0.5 && !isOn)) { tl in
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
                twitchStart = PurrClock.now()
            }
        }
        .onChange(of: externalHover) { _, newValue in
            if newValue {
                twitchStart = PurrClock.now()
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

    private var activeHover: Bool { isHovered || (allowSelfHover && selfHover) }

    var body: some View {
        let shouldAnimate = activeHover || (PurrClock.now() - hoverStartedAt < 0.65)
        TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: !shouldAnimate)) { _ in
            let now = PurrClock.now()
            let elapsed = now - hoverStartedAt
            let prog: Double = min(1.0, max(0.0, elapsed / 0.55))
            let wave: Double = activeHover ? sin(now * 6.5) : (elapsed < 0.65 ? sin(prog * .pi * 2.0) * (1.0 - prog) : 0.0)

            PurrCanvasView(size: CGSize(width: size, height: size)) { ctx, sz in
                drawIcon(ctx: &ctx, sz: sz, now: now, elapsed: elapsed, prog: prog, wave: wave, shouldAnimate: shouldAnimate)
            }
        }
        .frame(width: size, height: size)
        .onHover { inside in
            guard allowSelfHover else { return }
            selfHover = inside
            if inside {
                hoverStartedAt = PurrClock.now()
            }
        }
        .onChange(of: isHovered) { _, newVal in
            if newVal {
                hoverStartedAt = PurrClock.now()
            }
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
