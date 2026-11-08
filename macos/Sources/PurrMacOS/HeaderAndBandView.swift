import SwiftUI
import AppKit

// MARK: - Top Bar (.bar in PURR-desktop-v5.html)
struct PurrTopBarView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    @State private var hoveredMode: String? = nil

    var body: some View {
        HStack(spacing: 16) {
            // Traffic lights (.lights): Red = hide to tray (20-30MB standby!), Yellow = minimize, Green = zoom
            HStack(spacing: 8) {
                Circle()
                    .fill(Color(red: 1.0, green: 0x5F/255.0, blue: 0x57/255.0))
                    .frame(width: 12, height: 12)
                    .help("挂起到菜单栏托盘（释放窗口内存）")
                    .onTapGesture { store.onSuspendWindow?() }
                Circle()
                    .fill(Color(red: 0xFE/255.0, green: 0xBC/255.0, blue: 0x2E/255.0))
                    .frame(width: 12, height: 12)
                    .onTapGesture { NSApp.keyWindow?.miniaturize(nil) }
                Circle()
                    .fill(Color(red: 0x28/255.0, green: 0xC8/255.0, blue: 0x40/255.0))
                    .frame(width: 12, height: 12)
                    .onTapGesture { NSApp.keyWindow?.zoom(nil) }
            }

            // Brand (.brand)
            HStack(spacing: 9) {
                PurrMarkView(isAwake: store.isAwake, bg: theme.bg, ink: theme.ink)
                Text("PURR")
                    .font(.system(size: 15, weight: .black))
                    .tracking(4.2)
                    .foregroundColor(theme.ink)
            }

            Rectangle().fill(theme.line).frame(width: 1, height: 18)

            // Structured Status Bar (State Pill + Node + IP + Single-Cell Stacked ↑/↓ Speed)
            statusCluster

            Spacer(minLength: 10)

            // Expanded 走法 (.modes) — spacious 3-mode switcher now that redundant top-right takeover buttons are removed
            HStack(spacing: 10) {
                Text("走法")
                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                    .tracking(1.4)
                    .foregroundColor(theme.mute)

                HStack(spacing: 6) {
                    modeButton(key: "rule", label: "分流")
                    modeButton(key: "global", label: "全局")
                    modeButton(key: "direct", label: "直连")
                }
            }
        }
        .padding(.horizontal, 20)
        .frame(height: 50)
    }

    @ViewBuilder
    private var statusCluster: some View {
        let ip = store.exitInfo().ip
        HStack(spacing: 12) {
            // 1. State Badge
            if store.firstRun {
                stateBadge(dot: "○", text: "没有窝", inverted: false)
                Text("先导入订阅")
                    .font(.system(size: 11.5, design: .monospaced))
                    .foregroundColor(theme.mute)
            } else if let e = store.errKind {
                stateBadge(dot: "!", text: "卡住了", inverted: true)
                Text(store.troubleInfo(for: e).st)
                    .font(.system(size: 11.5, weight: .semibold, design: .monospaced))
                    .foregroundColor(theme.ink)
                ipChip(ip: ip)
            } else if store.isAwake {
                if store.mode == "direct" {
                    stateBadge(dot: "●", text: "直连中", inverted: true)
                    ipChip(ip: ip)
                    stackedSpeedCell
                } else {
                    let n = store.currentPrimaryNode
                    stateBadge(dot: "●", text: "呼噜中", inverted: true)
                    // 2. Node + Latency Pill (only once the core sent a real node)
                    if !store.groups.isEmpty {
                    Button(action: { store.page = 1 }) {
                        HStack(spacing: 5) {
                            Text(n.name)
                                .font(.system(size: 11.5, weight: .bold))
                                .foregroundColor(theme.ink)
                            Text(n.msText)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(theme.mute)
                        }
                        .padding(.horizontal, 7)
                        .padding(.vertical, 3)
                        .overlay(RoundedRectangle(cornerRadius: 3).stroke(theme.line, lineWidth: 1))
                    }
                    .buttonStyle(.plain)
                    }

                    // 3. IP Chip
                    ipChip(ip: ip)

                    // 4. Single-column 2-line Stacked Speed (Up on top, Down on bottom)
                    stackedSpeedCell
                }
            } else {
                stateBadge(dot: "z", text: "睡着了", inverted: false)
                ipChip(ip: ip)
            }
        }
    }

    private func stateBadge(dot: String, text: String, inverted: Bool) -> some View {
        HStack(spacing: 4) {
            Text(dot)
                .font(.system(size: 10, weight: .black, design: .monospaced))
            Text(text)
                .font(.system(size: 11.5, weight: .bold))
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 3)
        .background(inverted ? theme.ink : Color.clear)
        .foregroundColor(inverted ? theme.bg : theme.ink)
        .overlay(RoundedRectangle(cornerRadius: 3).stroke(theme.ink, lineWidth: 1.2))
        .clipShape(RoundedRectangle(cornerRadius: 3))
    }

    private func ipChip(ip: String) -> some View {
        Button(action: { store.showServiceDetail = true }) {
            HStack(spacing: 4) {
                Text("IP")
                    .font(.system(size: 10, weight: .bold, design: .monospaced))
                    .foregroundColor(theme.mute)
                Text(ip)
                    .font(.system(size: 11.5, weight: .medium, design: .monospaced))
                    .foregroundColor(theme.ink)
            }
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(theme.line, lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    /// Single compact cell with Upload on top (↑) and Download on bottom (↓)
    private var stackedSpeedCell: some View {
        VStack(alignment: .trailing, spacing: 0) {
            Text("↑ " + store.rateText(store.upSpeed))
                .font(.system(size: 9.5, weight: .medium, design: .monospaced))
                .foregroundColor(theme.mute)
            Text("↓ " + store.rateText(store.dnSpeed))
                .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                .foregroundColor(theme.ink)
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 2)
        .overlay(RoundedRectangle(cornerRadius: 3).stroke(theme.line, lineWidth: 1))
    }

    private func modeButton(key: String, label: String) -> some View {
        let active = store.mode == key
        let hov = hoveredMode == key || PurrSnapshot.forced("mode:\(key)")
        return Button(action: { store.switchMode(key) }) {
            HStack(spacing: 6) {
                PurrIconView(
                    name: key,
                    size: 17,
                    strokeWidth: 2.0,
                    isAwake: store.isAwake,
                    color: active ? theme.bg : (hov ? theme.ink : theme.mute),
                    isHovered: hov
                )
                Text(label)
                    .font(.system(size: 12.5, weight: active ? .bold : .semibold, design: .monospaced))
                    .tracking(0.8)
                    .foregroundColor(active ? theme.bg : (hov ? theme.ink : theme.mute))
            }
            .padding(.horizontal, 12)
            .frame(height: 28)
            .background(active ? theme.ink : (hov ? theme.ink.opacity(0.08) : Color.clear))
            .overlay(
                RoundedRectangle(cornerRadius: 4)
                    .stroke(active ? theme.ink : theme.ink.opacity(hov ? 0.5 : 0.25), lineWidth: 1.3)
            )
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .onHover { inside in hoveredMode = inside ? key : (hoveredMode == key ? nil : hoveredMode) }
    }
}

// MARK: - Ear Wave Band (.band in PURR-desktop-v5.html)
struct PurrEarBandView: View {
    @ObservedObject var store: PurrStore
    @ObservedObject private var vis = PurrVisibility.shared
    let theme: PurrTheme
    @State private var hoveredTab: Int? = nil

    static let earPts: [(Double, Double)] = [
        (-50, 0), (-33, -46), (-15, -19), (0, -22), (15, -19), (33, -46), (50, 0)
    ]

    static func earOffset(_ dx: Double) -> Double {
        if dx <= -50 || dx >= 50 { return 0 }
        // Smooth rounded dome across the crown between the two ears (-15 ... +15) instead of a sharp peak
        if dx >= -15 && dx <= 15 {
            return -19.0 - 3.2 * cos((dx / 15.0) * (.pi * 0.5))
        }
        for i in 0..<6 {
            let (a, ay) = earPts[i]
            let (b, by) = earPts[i + 1]
            if dx >= a && dx <= b {
                return ay + (by - ay) * (dx - a) / (b - a)
            }
        }
        return 0
    }

    var body: some View {
        GeometryReader { geo in
            let w = Double(geo.size.width)
            // Ear band: 30 fps only while something moves under the user's hand (ear sliding to a new
            // tab, hold-to-wake, flood); the awake breathing wave runs at 15 fps; asleep and settled =
            // no frames. Drawn by a display-link view, so a frame costs no SwiftUI update at all.
            let moveLeft = max(0.0, 0.6 - (PurrClock.now() - store.pageChangedAt))
            let bandStrokes: (CGSize, Double) -> [PurrStroke] = { size, nowSec in
                        var out: [PurrStroke] = []
                        let t = nowSec * 1000.0
                        let (earX, sp) = store.currentEarX(width: w, now: PurrClock.now())
                        let width = Double(size.width)
                        let y0 = 76.0
                        let hop = (1.0 - sp * 0.55) * (store.errKind != nil ? 0.5 : 1.0)
                        let stretch = 1.0 + sp * 0.6
                        let maxX = Int(width)

                        // Read the store once per frame, not once per point (~600 points, each read
                        // went through @Published getters and String compares).
                        let awake = store.isAwake
                        let hasErr = store.errKind != nil
                        let isGlobal = store.mode == "global"
                        let isDirect = store.mode == "direct"
                        let load = store.load
                        let holding = store.holding
                        let holdProgress = store.holdProgress
                        let br = 0.55 + 0.45 * sin(t / 900.0)

                        func yAt(_ xi: Int) -> CGFloat {
                            let x = Double(xi)
                            let dx = (x - earX) / stretch
                            var y = y0 + Self.earOffset(dx) * hop

                            if awake && hasErr {
                                let d = abs(x - earX)
                                if d > 46 {
                                    let f = max(0.0, 1.0 - (d - 46.0) / 320.0)
                                    let zig = 1.0 - abs(x.truncatingRemainder(dividingBy: 8.0) - 4.0) / 4.0
                                    y -= f * zig * (6.0 + 2.5 * sin(t / 60.0 + x * 0.7))
                                }
                            } else if awake {
                                let d = abs(x - earX) / width
                                var a = (1.1 + load * 4.5) * br * (1.0 - d * 0.5)
                                if isGlobal {
                                    a = a * 1.5 + 1.4
                                } else if isDirect {
                                    a = 0
                                } else {
                                    let near = max(0.0, 1.0 - abs(x - earX) / 80.0)
                                    let seg = min(1.0, max(0.0, (sin((x - t * 0.06) / 34.0) + 0.15) * 2.4))
                                    a *= max(near, seg)
                                }
                                if a != 0 {
                                    y += sin(x * 0.42 + t / 18.0) * a + sin(x * 0.11 - t / 60.0) * a * 0.6
                                }
                                if isDirect {
                                    y += sin(x * 0.018 - t / 800.0) * 2.0
                                }
                            } else if holding {
                                y += sin(x * 0.42 + t / 30.0) * holdProgress * 2.4
                            }
                            return CGFloat(y)
                        }

                        func buildSubpath(from f: Int, to endIdx: Int) -> Path {
                            let p = CGMutablePath()
                            guard f <= endIdx, f <= maxX else { return Path(p) }
                            let limit = min(endIdx, maxX)
                            p.move(to: CGPoint(x: CGFloat(f), y: yAt(f)))
                            var i = f + 2
                            while i < limit {
                                p.addLine(to: CGPoint(x: CGFloat(i), y: yAt(i)))
                                i += 2
                            }
                            p.addLine(to: CGPoint(x: CGFloat(limit), y: yAt(limit)))
                            return Path(p)
                        }

                        func strokeSegment(from f: Int, to endIdx: Int) {
                            let path = buildSubpath(from: f, to: endIdx)
                            if store.tunEnabled {
                                out.append(PurrStroke(path: path.cgPath, color: NSColor(theme.ink), width: 8.0, dash: []))
                                out.append(PurrStroke(path: path.cgPath, color: NSColor(theme.bg), width: 3.2, dash: []))
                            } else if store.isAwake && !store.sysProxy {
                                out.append(PurrStroke(path: path.cgPath, color: NSColor(theme.ink), width: 3.0, dash: [5, 8]))
                            } else {
                                out.append(PurrStroke(path: path.cgPath, color: NSColor(theme.ink), width: 3.0, dash: []))
                            }
                        }

                        let endIdx = maxX
                        if !store.isAwake && store.holdProgress > 0 {
                            let split = Int(round(width * store.holdProgress))
                            let warmPath = buildSubpath(from: 0, to: split)
                            out.append(PurrStroke(path: warmPath.cgPath, color: NSColor(theme.warm), width: 3.6, dash: []))
                            strokeSegment(from: split, to: endIdx)
                        } else if store.isAwake && store.holdProgress > 0 {
                            let split = Int(round(width * store.holdProgress))
                            let sleepPath = buildSubpath(from: 0, to: split)
                            out.append(PurrStroke(path: sleepPath.cgPath, color: NSColor(theme.ink.opacity(0.35)), width: 3.0, dash: []))
                            strokeSegment(from: split, to: endIdx)
                        } else {
                            strokeSegment(from: 0, to: endIdx)
                        }
                        return out
            }
            ZStack(alignment: .topLeading) {
                if PurrSnapshot.active {
                    TimelineView(.animation(minimumInterval: 1.0 / 30.0)) { tl in
                        PurrCanvasView(size: CGSize(width: geo.size.width, height: 136)) { ctx, size in
                            for st in bandStrokes(size, PurrClock.ref(tl.date)) { st.stroke(in: ctx.cg) }
                        }
                    }
                } else {
                    // Shape layers: a frame only hands Core Animation new paths, nothing is
                    // rasterized into a 2400x272 backing store here.
                    PurrStrokeCanvas(
                        size: CGSize(width: geo.size.width, height: 136),
                        interval: { [store] in
                            let moving = PurrClock.now() - store.pageChangedAt < 0.6
                            if store.holding || store.floodAnimating || moving { return 1.0 / 30.0 }
                            return store.isAwake ? 1.0 / 15.0 : nil
                        },
                        strokes: bandStrokes
                    )
                }

                // Tip label and grab area only move while the ear slides to a new tab.
                TimelineView(PurrFrames(
                    interval: 1.0 / 30.0,
                    until: Date().addingTimeInterval(moveLeft),
                    paused: !vis.mainWindowVisible || moveLeft <= 0
                )) { _ in
                    let (earX, _) = store.currentEarX(width: w, now: PurrClock.now())
                    ZStack(alignment: .topLeading) {
                        // Tip label (.tip)
                        let tipX = min(w - 115.0, max(120.0, earX + 92.0))
                        tipView
                            .position(x: tipX, y: 18)

                        // Grab area over the cat ears (#grab)
                        Color.white.opacity(0.001)
                            .frame(width: 120, height: 72)
                            .contentShape(Rectangle())
                            .position(x: earX, y: 44)
                            .gesture(
                                DragGesture(minimumDistance: 0)
                                    .onChanged { _ in store.beginHold() }
                                    .onEnded { _ in store.endHold() }
                            )
                    }
                }
            }

            // 6 Page Labels (.labels)
            VStack {
                Spacer()
                HStack(spacing: 0) {
                    ForEach(0..<store.pageTabs.count, id: \.self) { i in
                        let tab = store.pageTabs[i]
                        let isCur = store.page == i
                        let isHov = hoveredTab == i || PurrSnapshot.forced("tab:\(i)")
                        Button(action: {
                            store.page = i
                        }) {
                            VStack(spacing: 3) {
                                PurrIconView(
                                    name: tab.id,
                                    size: 22,
                                    strokeWidth: isCur ? 2.0 : 1.75,
                                    isAwake: store.isAwake,
                                    color: (isCur || isHov) ? theme.ink : theme.mute,
                                    isHovered: isHov
                                )
                                Text(tab.title)
                                    .font(.system(size: 14, weight: isCur ? .heavy : .semibold))
                                    .foregroundColor((isCur || isHov) ? theme.ink : theme.mute)
                            }
                            .frame(maxWidth: .infinity)
                            .frame(height: 44)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .onHover { inside in
                            hoveredTab = inside ? i : (hoveredTab == i ? nil : hoveredTab)
                        }
                    }
                }
                .padding(.bottom, 10)
            }
        }
        .frame(height: 136)
        .overlay(alignment: .top) {
            Rectangle().fill(theme.line).frame(height: 1)
        }
        .overlay(alignment: .bottom) {
            Rectangle().fill(theme.ink).frame(height: 1.5)
        }
    }

    @ViewBuilder
    private var tipView: some View {
        HStack(spacing: 4) {
            if store.firstRun {
                Text("先导入订阅，再按住耳朵")
            } else if store.holding {
                Text(store.isAwake ? "再按久一点…哄它睡觉" : "继续按住…它在醒了")
            } else {
                Text("按住耳朵 /")
                Text("Space")
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 3).stroke(store.nudgeTip ? theme.ink : theme.mute, lineWidth: 1))
                Text(store.isAwake ? "哄它睡觉" : "叫醒它")
            }
        }
        .font(.system(size: 10.5, weight: store.nudgeTip ? .bold : .regular, design: .monospaced))
        .tracking(0.8)
        .foregroundColor(store.nudgeTip ? theme.ink : theme.mute)
    }
}
