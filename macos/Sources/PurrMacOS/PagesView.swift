import SwiftUI

// MARK: - Shared Bracket Button (.bk in PURR-desktop-v5.html)
struct PurrBracketButton: View {
    let icon: String?
    let label: String
    let theme: PurrTheme
    let action: () -> Void
    @State private var hoverState = false
    @ObservedObject private var hoverBus = PurrHoverBus.shared
    private var hovering: Bool { hoverState || PurrSnapshot.forced("bk:\(label)") }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Text("[")
                if let ic = icon {
                    PurrIconView(name: ic, size: 14, strokeWidth: 2, isAwake: true, color: hovering ? theme.bg : theme.ink, isHovered: hovering)
                }
                Text(label)
                Text("]")
            }
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .tracking(0.5)
            .padding(.horizontal, 4)
            .padding(.vertical, 3)
            .background(hovering ? theme.ink : Color.clear)
            .foregroundColor(hovering ? theme.bg : theme.ink)
            .clipShape(RoundedRectangle(cornerRadius: 3))
        }
        .buttonStyle(.plain)
        .onHover { hoverState = $0 }
    }
}

// MARK: - Page 0: Overview (#p-ov in PURR-desktop-v5.html)
struct OverviewPageView: View {
    @ObservedObject var store: PurrStore
    @ObservedObject private var vis = PurrVisibility.shared
    let theme: PurrTheme
    @State private var hoverSysOverview: Bool = false
    @State private var hoverTunOverview: Bool = false

    var body: some View {
        if store.firstRun {
            FirstRunOverviewView(store: store, theme: theme)
        } else {
            VStack(spacing: 0) {
                // Top Row (300px height): Hero (left) + Stats 2x3 (right 340px)
                HStack(alignment: .top, spacing: 40) {
                    heroColumn
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

                    statsGrid
                        .frame(width: 340)
                        .frame(maxHeight: .infinity)
                }
                .frame(height: 300)

                // Bottom Row: Oscilloscope (left) + Side Today/Sub/Services (right 340px)
                HStack(alignment: .top, spacing: 40) {
                    scopeView
                        .frame(maxWidth: .infinity, maxHeight: .infinity)

                    sidePanel
                        .frame(width: 340)
                        .frame(maxHeight: .infinity, alignment: .topLeading)
                }
                .frame(maxHeight: .infinity)
            }
        }
    }

    private var heroColumn: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Caption (#cap)
            captionView
                .font(.system(size: 13))

            // Display area (.disp: zzz / prrr / trouble)
            ZStack(alignment: .bottomLeading) {
                if let err = store.errKind {
                    troubleView(kind: err)
                        .padding(.bottom, 10)
                } else if store.isAwake {
                    PurrHeroText(mode: store.mode, load: store.load, ink: theme.ink)
                        .allowsHitTesting(false)
                        .padding(.bottom, 8)
                        .transition(.opacity.combined(with: .move(edge: .bottom)))
                } else {
                    SleepingZzzView(ink: theme.ink)
                        .allowsHitTesting(false)
                        .padding(.bottom, 12)
                        .transition(.opacity.combined(with: .move(edge: .top)))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)

            // Exit row (.exit) + Prominent System Proxy & TUN Quick Toggles
            let ex = store.exitInfo()
            HStack(alignment: .center, spacing: 10) {
                Button(action: { store.showServiceDetail = true }) {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("当前 IP")
                            .font(.system(size: 11.5, weight: .bold, design: .monospaced))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(theme.ink)
                            .foregroundColor(theme.bg)
                            .clipShape(RoundedRectangle(cornerRadius: 3))
                        Text(ex.ip)
                            .font(.system(size: 14.5, weight: .bold, design: .monospaced))
                            .foregroundColor(theme.ink)
                        Text(ex.loc)
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundColor(theme.mute)
                            .lineLimit(1)
                    }
                }
                .buttonStyle(.plain)

                Spacer()

                // Fixed-width prominent buttons with larger animated icons (no text length change on click)
                Button(action: {
                    store.sysProxy.toggle()
                    store.showToast("系统代理" + (store.sysProxy ? " 已开启" : " 已关闭"))
                }) {
                    HStack(spacing: 7) {
                        PurrIconView(name: "sys", size: 20, strokeWidth: 1.95, isAwake: store.sysProxy, color: store.sysProxy ? theme.bg : theme.ink, isHovered: hoverSysOverview || PurrSnapshot.forced("ov:sys"))
                        Text("系统代理")
                            .font(.system(size: 13, weight: .bold, design: .monospaced))
                            .foregroundColor(store.sysProxy ? theme.bg : theme.ink)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(store.sysProxy ? theme.ink : theme.ink.opacity((hoverSysOverview || PurrSnapshot.forced("ov:sys")) ? 0.08 : 0.0))
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(theme.ink, lineWidth: 1.5))
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                }
                .buttonStyle(.plain)
                .onHover { hoverSysOverview = $0 }

                Button(action: {
                    store.toggleTun()
                }) {
                    HStack(spacing: 7) {
                        PurrIconView(name: "tun", size: 20, strokeWidth: 1.95, isAwake: store.tunEnabled, color: store.tunEnabled ? theme.bg : theme.ink, isHovered: hoverTunOverview || PurrSnapshot.forced("ov:tun"))
                        Text("虚拟网卡")
                            .font(.system(size: 13, weight: .bold, design: .monospaced))
                            .foregroundColor(store.tunEnabled ? theme.bg : theme.ink)
                        if !store.tunAuthorized && !store.tunEnabled {
                            Text(store.tunAuthorizing ? "授权中…" : "需授权")
                                .font(.system(size: 10, weight: .semibold, design: .monospaced))
                                .foregroundColor(theme.mute)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(store.tunEnabled ? theme.ink : theme.ink.opacity((hoverTunOverview || PurrSnapshot.forced("ov:tun")) ? 0.08 : 0.0))
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(theme.ink, lineWidth: 1.5))
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                }
                .buttonStyle(.plain)
                .help(store.tunAuthorized ? "虚拟网卡（TUN）：接管所有程序的流量" : PurrStore.tunUnauthorizedText)
                .disabled(store.tunAuthorizing)
                .onHover { hoverTunOverview = $0 }
            }
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .top) {
                Rectangle().fill(theme.line).frame(height: 1)
            }

            // 4 Route Cards (.routes)
            HStack(spacing: 8) {
                ForEach(store.groups.indices.filter { store.nodes.indices.contains(store.groups[$0].selected) }, id: \.self) { gi in
                    let g = store.groups[gi]
                    let n = store.nodes[g.selected]
                    let isBad = store.errKind == "timeout" && g.selected == store.badNodeId
                    RouteCardButton(
                        groupName: g.name,
                        nodeName: n.name,
                        latencyText: isBad ? "超时" : (store.errKind != nil ? "—" : n.msText + (n.verdict.level == 2 ? " · 抖" : "")),
                        isBad: isBad,
                        dimmed: store.isAwake && store.mode == "direct",
                        theme: theme
                    ) {
                        store.selectedGroupIdx = gi
                        store.page = 1
                    }
                }
            }
            .padding(.bottom, 14)
        }
    }

    @ViewBuilder
    private var captionView: some View {
        let n = store.currentPrimaryNode.name
        HStack(spacing: 4) {
            if store.errKind != nil {
                Text("\(store.modeLabel)：")
                    .fontWeight(.bold)
                    .foregroundColor(theme.ink)
                Text("醒着，但 ")
                    .foregroundColor(theme.mute)
                Text("流量过不去")
                    .fontWeight(.bold)
                    .underline()
                    .foregroundColor(theme.ink)
            } else if store.isAwake && store.groups.isEmpty && store.mode != "direct" {
                Text(store.modeLabel).fontWeight(.bold).foregroundColor(theme.ink)
                Text(store.statsReady ? "：内核还没有给出节点。" : "：内核还没有回应，暂时拿不到节点。").foregroundColor(theme.mute)
            } else if store.isAwake {
                switch store.mode {
                case "global":
                    Text("全局").fontWeight(.bold).foregroundColor(theme.ink)
                    Text("：所有流量都经由").foregroundColor(theme.mute)
                    Text(n).fontWeight(.bold).foregroundColor(theme.ink)
                    Text("。").foregroundColor(theme.mute)
                case "direct":
                    Text("直连").fontWeight(.bold).foregroundColor(theme.ink)
                    Text("：醒着，但不带路，流量全部直走。").foregroundColor(theme.mute)
                default:
                    Text("分流").fontWeight(.bold).foregroundColor(theme.ink)
                    Text("：国内直走，其余经由").foregroundColor(theme.mute)
                    Text(n).fontWeight(.bold).foregroundColor(theme.ink)
                    Text("。").foregroundColor(theme.mute)
                }
                if !store.sysProxy && !store.tunEnabled {
                    Text("但系统代理和 TUN 都关着，还没有流量经过它。")
                        .fontWeight(.bold)
                        .underline()
                        .foregroundColor(theme.ink)
                }
            } else {
                Text("睡着了：系统代理已还原，内核仍在 \(store.mixedPort) 端口待命。醒来后按").foregroundColor(theme.mute)
                Text(store.modeLabel).fontWeight(.bold).foregroundColor(theme.ink)
                Text("走。").foregroundColor(theme.mute)
            }
        }
    }

    private func troubleView(kind: String) -> some View {
        let info = store.troubleInfo(for: kind)
        return HStack(alignment: .center, spacing: 20) {
            TroubleFaceView(kind: kind, ink: theme.ink, bg: theme.bg)
                .frame(width: 248, height: 168)

            VStack(alignment: .leading, spacing: 6) {
                Text(info.k)
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(1.6)
                    .foregroundColor(theme.mute)
                Text(info.h)
                    .font(.system(size: 30, weight: .black))
                    .foregroundColor(theme.ink)
                Text(info.p)
                    .font(.system(size: 13))
                    .foregroundColor(theme.mute)
                    .fixedSize(horizontal: false, vertical: true)

                HStack(spacing: 10) {
                    ForEach(Array(info.actions.enumerated()), id: \.offset) { idx, act in
                        Button(action: { store.performTroubleFix(act.1) }) {
                            Text(act.0)
                                .font(.system(size: 12, weight: .semibold, design: .monospaced))
                                .padding(.horizontal, 12)
                                .padding(.vertical, 5)
                                .background(idx == 0 ? theme.ink : Color.clear)
                                .foregroundColor(idx == 0 ? theme.bg : theme.ink)
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(theme.ink, lineWidth: 1.5))
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                        }
                        .buttonStyle(.plain)
                    }
                    if kind == "timeout" && store.troubleCountdown > 0 {
                        Text("换个窝继续睡 已开启 · \(store.troubleCountdown) 秒后自动换")
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                    }
                }
                .padding(.top, 6)
            }
        }
    }

    private var statsGrid: some View {
        // Only connections that really go through a node (REJECT never does).
        let pxCount = store.conns.filter { $0.pol != "DIRECT" && $0.pol != "REJECT" }.count
        let drCount = store.conns.filter { $0.pol == "DIRECT" }.count
        let today = (store.todayTotal ?? "—").split(separator: " ", maxSplits: 1).map(String.init)
        let peak = { (v: Double) -> String in store.histCount > 0 ? store.rateText(v) : "—" }
        return VStack(spacing: 0) {
            HStack(spacing: 20) {
                statCell(label: "DOWN · 下行网速 ↓", value: store.rateParts(store.dnSpeed).0, unit: store.rateParts(store.dnSpeed).1, sub: "60 秒峰值 " + peak(store.peakDn), showBottomBorder: true)
                statCell(label: "UP · 上行网速 ↑", value: store.rateParts(store.upSpeed).0, unit: store.rateParts(store.upSpeed).1, sub: "60 秒峰值 " + peak(store.peakUp), showBottomBorder: true)
            }
            .frame(maxHeight: .infinity)

            HStack(spacing: 20) {
                statCell(
                    label: store.isAwake ? (store.mode == "direct" ? "AWAKE" : "PURRING") : "LAST NAP",
                    value: store.elapsedStr,
                    unit: nil,
                    sub: store.isAwake ? "本次连接" : "上次睡前呼噜了这么久",
                    showBottomBorder: true
                )
                statCell(
                    label: "TODAY",
                    value: today.first ?? "—",
                    unit: today.count > 1 ? today[1] : nil,
                    sub: "↓ \(store.todayDown ?? "—") · ↑ \(store.todayUp ?? "—")",
                    showBottomBorder: true
                )
            }
            .frame(maxHeight: .infinity)

            HStack(spacing: 20) {
                statCell(
                    label: "CONNECTIONS",
                    value: "\(store.conns.count)",
                    unit: "条",
                    sub: store.isAwake ? "经过窝点 \(pxCount) · 直连 \(drCount)" : "睡着时没有连接",
                    showBottomBorder: false
                )
                memoryCell
            }
            .frame(maxHeight: .infinity)
        }
        .padding(.leading, 24)
        .overlay(alignment: .leading) {
            Rectangle().fill(theme.ink).frame(width: 1.5)
        }
    }

    private func statCell(label: String, value: String, unit: String?, sub: String, showBottomBorder: Bool) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.system(size: 11, design: .monospaced))
                .tracking(1.6)
                .foregroundColor(theme.mute)
            HStack(alignment: .firstTextBaseline, spacing: 5) {
                Text(value)
                    .font(.system(size: 28, weight: .bold, design: .monospaced))
                    .foregroundColor(theme.ink)
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                if let u = unit {
                    Text(u)
                        .font(.system(size: 12, weight: .medium, design: .monospaced))
                        .foregroundColor(theme.mute)
                }
            }
            Text(sub)
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(theme.mute)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .overlay(alignment: .bottom) {
            if showBottomBorder {
                Rectangle().fill(theme.line).frame(height: 1)
            }
        }
    }

    private var memoryCell: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text("MEMORY · 内存")
                .font(.system(size: 11, design: .monospaced))
                .tracking(1.6)
                .foregroundColor(theme.mute)
            HStack(alignment: .firstTextBaseline, spacing: 5) {
                Text(store.memMB.map { String(format: "%.1f", $0) } ?? "—")
                    .font(.system(size: 28, weight: .bold, design: .monospaced))
                    .foregroundColor(theme.ink)
                Text("MB")
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .foregroundColor(theme.mute)
            }
            // Measured split (Purr / core, decimal MB) and measured CPU; no invented limit.
            let mb = { (v: Double?) -> String in v.map { String(format: "%.0f", $0) } ?? "—" }
            Text("Purr \(mb(store.memShellMB)) · 内核 \(mb(store.memCoreMB)) MB")
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(theme.mute)
                .lineLimit(1)
            if let cpu = store.cpuPct {
                Text(String(format: "CPU %.1f%%", cpu))
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(theme.mute)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
    }

    private var scopeView: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("TRAFFIC · LAST 60S")
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(1.6)
                    .foregroundColor(theme.mute)
                Spacer()
                HStack(spacing: 14) {
                    HStack(spacing: 6) {
                        Rectangle().fill(theme.ink).frame(width: 16, height: 2)
                        Text("下行")
                    }
                    HStack(spacing: 6) {
                        Rectangle().fill(theme.ink).frame(width: 16, height: 1.5)
                        Text("上行")
                    }
                }
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(theme.mute)
                .padding(.trailing, 16)

                Text("PEAK " + (store.histCount > 0 ? store.rateText(store.peakDn).uppercased() : "—"))
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(1.6)
                    .foregroundColor(theme.mute)
            }

            GeometryReader { scopeGeo in
                // Data lines are drawn exactly at their values: zero traffic is flat on the 0 line.
                Group {
                    PurrCanvasView(size: scopeGeo.size) { ctx, size in
                        let w = size.width
                        let h = size.height
                        let pk = store.histDn.max() ?? 0
                        let mx = max(5.0, ceil(pk / 5.0) * 5.0)
                        let chartW = max(10, w - 58)
                        let top: CGFloat = 8
                        let bot: CGFloat = h - 20
                        func yPos(_ v: Double) -> CGFloat {
                            bot - CGFloat(v / mx) * (bot - top)
                        }

                        for v in [0.0, mx / 2.0, mx] {
                            let y = yPos(v)
                            var grid = Path()
                            grid.move(to: CGPoint(x: 0, y: y))
                            grid.addLine(to: CGPoint(x: chartW, y: y))
                            ctx.stroke(grid, with: .color(theme.line), lineWidth: 1)
                            ctx.drawText("\(Int(v)) MB/s", color: theme.mute, at: CGPoint(x: chartW + 30, y: y))
                        }

                        ctx.drawText("−60s", color: theme.mute, at: CGPoint(x: 16, y: h - 6))
                        ctx.drawText("−30s", color: theme.mute, at: CGPoint(x: chartW / 2, y: h - 6))
                        ctx.drawText("现在", color: theme.mute, at: CGPoint(x: chartW - 14, y: h - 6))

                        func drawSeries(_ arr: [Double], dashed: Bool, lw: CGFloat) {
                            guard arr.count > 1 else { return }
                            var p = Path()
                            for (j, v) in arr.enumerated() {
                                let x = CGFloat(j) / CGFloat(arr.count - 1) * chartW
                                let y = max(top, min(bot, yPos(v)))
                                if j == 0 { p.move(to: CGPoint(x: x, y: y)) }
                                else { p.addLine(to: CGPoint(x: x, y: y)) }
                            }
                            if dashed {
                                ctx.stroke(p, with: .color(theme.ink), style: StrokeStyle(lineWidth: lw, lineJoin: .round, dash: [4, 4]))
                            } else {
                                ctx.stroke(p, with: .color(theme.ink), style: StrokeStyle(lineWidth: lw, lineJoin: .round))
                            }
                        }

                        drawSeries(store.histUp, dashed: true, lw: 1.5)
                        drawSeries(store.histDn, dashed: false, lw: 2.2)
                    }
                }
            }
            .frame(height: 170)
        }
        .padding(.top, 14)
        .overlay(alignment: .top) {
            Rectangle().fill(theme.ink).frame(height: 1.5)
        }
    }

    private var sidePanel: some View {
        let pp = store.routeProxyFraction ?? 0
        let activeSub = store.subscriptions.first(where: { $0.active }) ?? store.subscriptions.first
        let isSubErr = store.errKind == "sub"

        return VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 8) {
                Text("TODAY · 去向")
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(1.6)
                    .foregroundColor(theme.mute)

                GeometryReader { g in
                    ZStack(alignment: .leading) {
                        PurrCanvasView(size: CGSize(width: g.size.width, height: 14)) { ctx, sz in
                            var x: CGFloat = -20
                            while x < sz.width + 20 {
                                var p = Path()
                                p.move(to: CGPoint(x: x, y: sz.height))
                                p.addLine(to: CGPoint(x: x + sz.height, y: 0))
                                ctx.stroke(p, with: .color(theme.line), lineWidth: 1.5)
                                x += 6
                            }
                        }
                        Rectangle()
                            .fill(theme.ink)
                            .frame(width: g.size.width * CGFloat(pp))
                    }
                    .overlay(Rectangle().stroke(theme.ink, lineWidth: 1.5))
                }
                .frame(height: 14)

                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text("■ 经过窝点 ")
                            .foregroundColor(theme.mute)
                        + Text(store.routeProxy ?? "—")
                            .fontWeight(.bold).foregroundColor(theme.ink)
                        + Text(store.routeProxyFraction != nil ? " · \(Int(round(pp * 100)))%" : " · —")
                            .foregroundColor(theme.mute)

                        Spacer()

                        Text("▨ 直连 ")
                            .foregroundColor(theme.mute)
                        + Text(store.routeDirect ?? "—")
                            .fontWeight(.bold).foregroundColor(theme.ink)
                        + Text(store.routeProxyFraction != nil ? " · \(Int(round((1 - pp) * 100)))%" : " · —")
                            .foregroundColor(theme.mute)
                    }
                    Text("抓到 ")
                        .foregroundColor(theme.mute)
                    + Text(store.blockedMice.map { "\($0)" } ?? "—")
                        .fontWeight(.bold).foregroundColor(theme.ink)
                    + Text(" 只老鼠（广告 / 追踪）")
                        .foregroundColor(theme.mute)
                }
                .font(.system(size: 11, design: .monospaced))
            }

            HStack(alignment: .firstTextBaseline) {
                HStack(spacing: 6) {
                    Text("YARN · 订阅")
                        .font(.system(size: 11, design: .monospaced))
                        .tracking(1.6)
                        .foregroundColor(theme.mute)
                    Text(activeSub?.name ?? "未导入订阅")
                        .font(.system(size: 13, weight: .bold))
                        .foregroundColor(theme.ink)
                }
                Spacer()
                if isSubErr {
                    Text("⚠ 已过期 · 窝点全部停用")
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundColor(theme.ink)
                } else if let sub = activeSub {
                    Text("\(sub.leftText) · \(sub.expiryText)")
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundColor(theme.mute)
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                }
            }
            .padding(.top, 6)
            .overlay(alignment: .top) {
                Rectangle().fill(theme.line).frame(height: 1)
            }

            // SERVICES · 常用网站：mihomo 按每个网站实际分流真测（PurrStore.probeServices），
            // 停在概览时每 60 秒自动重测；没测到的显示「—」。概览只放前 4 个（2x2），
            // 其余收进「+N ▸」，点开原详情弹窗看全部（8 个都测）。不加行，窗口高度不变。
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text("SERVICES · 常用网站")
                        .font(.system(size: 11, design: .monospaced))
                        .tracking(1.6)
                        .foregroundColor(theme.mute)
                    Spacer()
                    if store.isTestingServices {
                        Text("检测中…")
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                    }
                    if store.serviceProbes.count > 4 {
                        Text("+\(store.serviceProbes.count - 4) ▸")
                            .font(.system(size: 11, weight: .bold, design: .monospaced))
                            .foregroundColor(theme.ink)
                    }
                }
                LazyVGrid(columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)], spacing: 4) {
                    ForEach(Array(store.serviceProbes.prefix(4))) { svc in
                        HStack(spacing: 6) {
                            Text(svc.name)
                                .font(.system(size: 12, weight: .bold))
                                .foregroundColor(theme.ink)
                                .lineLimit(1)
                            Spacer(minLength: 4)
                            Circle()
                                .fill(theme.ink)
                                .frame(width: 5, height: 5)
                                .opacity(svc.ms > 0 ? 0.9 : 0.3)
                            Text(svc.msText)
                                .font(.system(size: 12, weight: .bold, design: .monospaced))
                                .foregroundColor(svc.ms > 0 ? theme.ink : theme.mute)
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .overlay(RoundedRectangle(cornerRadius: 4).stroke(theme.line, lineWidth: 1))
                    }
                }
            }
            .contentShape(Rectangle())
            .onTapGesture { store.showServiceDetail = true }
        }
        .padding(.top, 12)
        .padding(.leading, 24)
        .overlay(alignment: .top) {
            Rectangle().fill(theme.ink).frame(height: 1.5)
        }
        .overlay(alignment: .leading) {
            Rectangle().fill(theme.ink).frame(width: 1.5)
        }
    }
}

// MARK: - Service Status & Exit IP Diagnostics Modal
struct ServiceDetailModalView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    var body: some View {
        let ex = store.exitInfo()
        ZStack {
            Color.black.opacity(0.35)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .ignoresSafeArea()
                .contentShape(Rectangle())
                .onTapGesture { store.showServiceDetail = false }

            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("SERVICE STATUS & IP · 服务状态与出口详情")
                            .font(.system(size: 11, design: .monospaced))
                            .tracking(1.5)
                            .foregroundColor(theme.mute)
                        Text("连通性与当前 IP 诊断")
                            .font(.system(size: 24, weight: .black))
                            .foregroundColor(theme.ink)
                    }
                    Spacer()
                    PurrBracketButton(icon: nil, label: "复制终端代理", theme: theme) {
                        store.copyTerminalProxyCommand()
                    }
                    PurrBracketButton(icon: nil, label: "刷新 DNS", theme: theme) {
                        store.flushDnsCache()
                    }
                    PurrBracketButton(icon: "test", label: store.isTestingServices ? "检测中…" : "重新测速", theme: theme) {
                        store.retestServices()
                    }
                    PurrBracketButton(icon: nil, label: "关闭", theme: theme) {
                        store.showServiceDetail = false
                    }
                }

                // Dual IP Display: Local Direct IP vs Proxy Exit IP
                HStack(spacing: 14) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("PROXY EXIT · 代理出口 IP")
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                        Text(store.isAwake && store.mode != "direct" ? ex.ip : "未启用（当前直连）")
                            .font(.system(size: 18, weight: .bold, design: .monospaced))
                            .foregroundColor(theme.ink)
                        Text(store.isAwake && store.mode != "direct" ? ex.loc : "叫醒猫并切到分流/全局后生效")
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(theme.ink, lineWidth: 1.5))

                    VStack(alignment: .leading, spacing: 4) {
                        Text("LOCAL DIRECT · 本地宽带 IP")
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                        Text(store.directIpText)
                            .font(.system(size: 18, weight: .bold, design: .monospaced))
                            .foregroundColor(theme.ink)
                        Text(store.directLocText)
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(theme.line, lineWidth: 1.2))
                }

                // 2x4 Grid of Common Websites & Services Latency
                VStack(alignment: .leading, spacing: 8) {
                    Text("COMMON SERVICES · 常用网站")
                        .font(.system(size: 10.5, design: .monospaced))
                        .tracking(1.4)
                        .foregroundColor(theme.mute)

                    LazyVGrid(columns: [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)], spacing: 10) {
                        ForEach(store.serviceProbes) { svc in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(svc.name)
                                        .font(.system(size: 14, weight: .bold))
                                        .foregroundColor(theme.ink)
                                    Text(svc.routeGroup.isEmpty ? svc.domain : "\(svc.domain) · \(svc.routeGroup)")
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundColor(theme.mute)
                                }
                                Spacer()
                                HStack(spacing: 6) {
                                    Circle()
                                        .fill(theme.ink)
                                        .frame(width: 6, height: 6)
                                    Text(svc.msText)
                                        .font(.system(size: 14, weight: .bold, design: .monospaced))
                                        .foregroundColor(theme.ink)
                                }
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 9)
                            .overlay(RoundedRectangle(cornerRadius: 4).stroke(theme.line, lineWidth: 1))
                        }
                    }
                }
            }
            .padding(24)
            .frame(width: 660)
            .background(theme.bg)
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(theme.ink, lineWidth: 2))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .shadow(color: Color.black.opacity(0.18), radius: 18, x: 0, y: 8)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .ignoresSafeArea()
    }
}

// MARK: - Sleeping ZZZ & Purring PRRRR Hero Views
struct SleepingZzzView: View {
    let ink: Color
    @ObservedObject private var vis = PurrVisibility.shared

    var body: some View {
        if PurrSnapshot.active {
            TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: !vis.mainWindowVisible)) { tl in
                let t = PurrClock.ref(tl.date)
                HStack(alignment: .bottom, spacing: 10) {
                    Text("z")
                        .font(.system(size: 56, weight: .black))
                        .foregroundColor(ink.opacity(0.25))
                        .offset(y: CGFloat(sin(t * 1.25) * 4.0))
                    Text("z")
                        .font(.system(size: 92, weight: .black))
                        .foregroundColor(ink.opacity(0.50))
                        .offset(y: CGFloat(sin(t * 1.25 - 1.6) * 6.0))
                    Text("z")
                        .font(.system(size: 136, weight: .black))
                        .foregroundColor(ink)
                        .offset(y: CGFloat(sin(t * 1.25 - 3.2) * 8.0))
                }
            }
        } else {
            // Slow 5 s bob, drawn at 30 fps.
            let row = PurrGlyphRow(
                chars: ["z", "z", "z"],
                fontSizes: [56, 92, 136],
                colors: [NSColor(ink.opacity(0.25)), NSColor(ink.opacity(0.50)), NSColor(ink)],
                spacing: 10
            )
            let m: CGFloat = 12
            PurrGlyphCanvas(
                row: row,
                margin: m,
                interval: { 1.0 / 30.0 },
                offsets: { i, t in CGFloat(sin(t * 1.25 - 1.6 * Double(i)) * (4.0 + 2.0 * Double(i))) }
            )
            .padding(-m)
        }
    }
}

struct PurrHeroText: View {
    let mode: String
    @ObservedObject private var vis = PurrVisibility.shared
    let load: Double
    let ink: Color

    var body: some View {
        let chars: [String] = mode == "global" ? ["P","R","R","R","R"] : (mode == "direct" ? ["m","r","r","p","."] : ["p","r","r","r","r"])
        let opacities: [Double] = mode == "rule" ? [1.0, 1.0, 1.0, 0.55, 0.25] : [1.0, 1.0, 1.0, 1.0, 1.0]
        let fontSize: CGFloat = mode == "global" ? 116 : 136
        let amp = 1.5 + load * 4.0
        let freq = mode == "global" ? 100.0 : 70.0
        let spacing: CGFloat = mode == "global" ? 8 : 4

        if PurrSnapshot.active {
            TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: mode == "direct" || !vis.mainWindowVisible)) { tl in
                let t = PurrClock.ref(tl.date)
                HStack(alignment: .bottom, spacing: spacing) {
                    ForEach(0..<chars.count, id: \.self) { i in
                        let dy = (i > 0 && mode != "direct") ? CGFloat(sin(t * freq + Double(i) * 2.1) * amp) : 0
                        Text(chars[i])
                            .font(.system(size: fontSize, weight: .black))
                            .foregroundColor(ink.opacity(opacities[i]))
                            .offset(y: dy)
                    }
                }
            }
        } else {
            // The purr jitter is an ~11-16 Hz shake, already beyond what 30 fps can follow; 15 fps
            // reads the same at half the frames.
            let row = PurrGlyphRow(
                chars: chars,
                fontSizes: [fontSize],
                colors: opacities.map { NSColor(ink.opacity($0)) },
                spacing: spacing
            )
            let m: CGFloat = 8
            let direct = mode == "direct"
            PurrGlyphCanvas(
                row: row,
                margin: m,
                interval: { direct ? nil : 1.0 / 30.0 },
                offsets: { i, t in (i > 0 && !direct) ? CGFloat(sin(t * freq + Double(i) * 2.1) * amp) : 0 }
            )
            .padding(-m)
        }
    }
}

struct RouteCardButton: View {
    let groupName: String
    let nodeName: String
    let latencyText: String
    let isBad: Bool
    let dimmed: Bool
    let theme: PurrTheme
    let action: () -> Void
    @State private var hoverState = false
    @ObservedObject private var hoverBus = PurrHoverBus.shared
    private var hovering: Bool { hoverState || PurrSnapshot.forced("route:\(groupName)") }

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 2) {
                Text(groupName)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundColor(hovering ? theme.bg.opacity(0.75) : theme.mute)
                HStack(alignment: .firstTextBaseline) {
                    Text(nodeName)
                        .font(.system(size: 13, weight: .bold))
                        .foregroundColor(hovering ? theme.bg : theme.ink)
                        .lineLimit(1)
                    Spacer()
                    Text(latencyText)
                        .font(.system(size: 11, weight: isBad ? .bold : .regular, design: .monospaced))
                        .foregroundColor(hovering ? theme.bg.opacity(0.75) : (isBad ? theme.ink : theme.mute))
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(hovering ? theme.ink : Color.clear)
            .overlay(
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(theme.ink, style: StrokeStyle(lineWidth: 1.5, dash: isBad ? [4, 3] : []))
            )
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .opacity(dimmed ? 0.35 : 1.0)
        }
        .buttonStyle(.plain)
        .onHover { hoverState = $0 }
    }
}

// MARK: - First Run Welcome View (.fr in PURR-desktop-v5.html)
struct FirstRunOverviewView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    var body: some View {
        HStack(alignment: .top, spacing: 48) {
            VStack(alignment: .leading, spacing: 0) {
                Text("WELCOME · 第一次见面")
                    .font(.system(size: 11, design: .monospaced))
                    .tracking(1.6)
                    .foregroundColor(theme.mute)
                Text("还没有窝。")
                    .font(.system(size: 72, weight: .black))
                    .foregroundColor(theme.ink)
                    .padding(.vertical, 10)
                Text("PURR 是一只带路的猫。给它一个 订阅（毛线团），它就知道往哪儿走。")
                    .font(.system(size: 14))
                    .foregroundColor(theme.mute)
                    .padding(.bottom, 24)

                VStack(spacing: 0) {
                    // Step 01
                    HStack(alignment: .top, spacing: 18) {
                        Text("01")
                            .font(.system(size: 12, weight: .bold, design: .monospaced))
                            .padding(.horizontal, 5)
                            .padding(.vertical, 2)
                            .background(theme.ink)
                            .foregroundColor(theme.bg)
                            .clipShape(RoundedRectangle(cornerRadius: 3))
                        VStack(alignment: .leading, spacing: 6) {
                            Text("粘贴订阅链接").font(.system(size: 15, weight: .heavy)).foregroundColor(theme.ink)
                            Text("机场后台一般叫“订阅地址”或“Clash 订阅”").font(.system(size: 12.5)).foregroundColor(theme.mute)
                            HStack(spacing: 12) {
                                TextField("https://…/api/v1/client/subscribe?token=…", text: $store.firstRunUrl)
                                    .textFieldStyle(.plain)
                                    .font(.system(size: 12.5, design: .monospaced))
                                    .padding(.vertical, 6)
                                    .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }
                                Button(action: { store.performImport() }) {
                                    Text(store.isImporting ? "闻一闻…" : "导入")
                                        .font(.system(size: 13, weight: .bold))
                                        .padding(.horizontal, 14)
                                        .padding(.vertical, 6)
                                        .background(theme.ink)
                                        .foregroundColor(theme.bg)
                                        .clipShape(RoundedRectangle(cornerRadius: 4))
                                }
                                .buttonStyle(.plain)
                            }
                            // Three quick 6 px pushes to the left in 0.3 s, then back at rest.
                            .modifier(PurrShakeEffect(phase: store.shakeCount * 3))
                            .animation(.linear(duration: 0.3), value: store.shakeCount)
                        }
                    }
                    .padding(.vertical, 14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(alignment: .top) { Rectangle().fill(theme.ink).frame(height: 1.5) }
                    .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }

                    // Step 02
                    HStack(alignment: .top, spacing: 18) {
                        Text("02").font(.system(size: 12, weight: .bold, design: .monospaced)).foregroundColor(theme.ink)
                        VStack(alignment: .leading, spacing: 4) {
                            Text("按住耳朵，叫醒它").font(.system(size: 15, weight: .heavy)).foregroundColor(theme.ink)
                            Text("按住上面那对耳朵约 1 秒，或者按住 Space").font(.system(size: 12.5)).foregroundColor(theme.mute)
                        }
                    }
                    .padding(.vertical, 14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }

                    // Step 03
                    HStack(alignment: .top, spacing: 18) {
                        Text("03").font(.system(size: 12, weight: .bold, design: .monospaced)).foregroundColor(theme.ink)
                        VStack(alignment: .leading, spacing: 4) {
                            Text("挑个走法").font(.system(size: 15, weight: .heavy)).foregroundColor(theme.ink)
                            Text("大多数时候选 分流 就好：国内直走，其余经由窝点").font(.system(size: 12.5)).foregroundColor(theme.mute)
                        }
                    }
                    .padding(.vertical, 14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }
                }
            }
            .frame(maxWidth: .infinity)

            VStack(spacing: 12) {
                DenseAsciiCatView(kind: "empty", ink: theme.ink, bg: theme.bg)
                    .frame(width: 240, height: 184)
                    .padding(.bottom, 2)
                Text("z z z")
                    .font(.system(size: 22, weight: .heavy, design: .monospaced))
                    .foregroundColor(theme.ink)
                Text("它在等一个窝")
                    .font(.system(size: 12.5))
                    .foregroundColor(theme.mute)

                VStack(alignment: .leading, spacing: 6) {
                    Text("导入后它会：")
                    Text("01  读取 全部窝点")
                    Text("02  挨个闻一闻（测速）")
                    Text("03  挑 最暖的 那个待命")
                }
                .font(.system(size: 11.5, design: .monospaced))
                .foregroundColor(theme.mute)
                .padding(.top, 12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .overlay(alignment: .top) { Rectangle().fill(theme.ink).frame(height: 1.5) }
            }
            .frame(width: 300)
            .padding(.top, 20)
        }
    }
}

// MARK: - Page 1: Nodes & Latency Spectrum (#p-px in PURR-desktop-v5.html)
struct NodesPageView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    /// 刻度自适应：右端 = max(250, 最慢已测节点 × 1.08 向上取整到 50)（Win `spectrum_top` 同式）
    static func spectrumTop(_ slowest: Int) -> Double {
        guard slowest > 0 else { return 250 }
        return max(250, (Double(slowest) * 1.08 / 50).rounded(.up) * 50)
    }
    private func logX(_ ms: Double, top: Double = 250) -> CGFloat {
        let clamped = min(top, max(15.0, ms))
        let lin = (clamped - 15.0) / (top - 15.0)
        let lg = log(clamped / 15.0) / log(top / 15.0)
        let v = 0.62 * lin + 0.38 * lg
        return CGFloat(min(0.95, max(0.03, v * 0.92 + 0.03)))
    }

    var body: some View {
        if store.groups.indices.contains(store.selectedGroupIdx),
           store.nodes.indices.contains(store.groups[store.selectedGroupIdx].selected) {
            content
        } else {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("窝点").font(.system(size: 34, weight: .black)).foregroundColor(theme.ink)
                    Text("挑个暖和的窝").font(.system(size: 13, weight: .medium)).foregroundColor(theme.mute)
                }
                Text(store.statsReady ? "内核还没有给出节点。" : "内核还没有回应，暂时拿不到节点。")
                    .font(.system(size: 14))
                    .foregroundColor(theme.mute)
                    .padding(.top, 36)
                Spacer()
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
    }

    @ViewBuilder
    private var content: some View {
        let g = store.groups[store.selectedGroupIdx]
        let selNode = store.nodes[g.selected]
        let detailNodeId = store.hoveredNodeId ?? g.selected
        let detailNode = store.nodes[detailNodeId]

        VStack(alignment: .leading, spacing: 0) {
            // Header (.ph)
            HStack(alignment: .firstTextBaseline, spacing: 24) {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("窝点").font(.system(size: 34, weight: .black)).foregroundColor(theme.ink)
                    Text("挑个暖和的窝").font(.system(size: 13, weight: .medium)).foregroundColor(theme.mute)
                }

                HStack(spacing: 2) {
                    ForEach(store.groups.indices, id: \.self) { i in
                        let active = store.selectedGroupIdx == i
                        Button(action: { store.selectedGroupIdx = i }) {
                            Text(store.groups[i].name)
                                .font(.system(size: 12, design: .monospaced))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(active ? theme.ink : Color.clear)
                                .foregroundColor(active ? theme.bg : theme.mute)
                                .clipShape(RoundedRectangle(cornerRadius: 3))
                        }
                        .buttonStyle(.plain)
                    }
                }

                Spacer()

                HStack(alignment: .firstTextBaseline, spacing: 16) {
                    HStack(spacing: 4) {
                        Text("当前").foregroundColor(theme.mute)
                        Text("\(g.name) → \(selNode.name)").fontWeight(.bold).foregroundColor(theme.ink)
                    }
                    .font(.system(size: 12, design: .monospaced))

                    PurrBracketButton(
                        icon: "test",
                        label: store.isTestingNodes ? "闻一闻…" : "测速",
                        theme: theme
                    ) {
                        store.triggerSpeedTest()
                    }
                }
            }
            .padding(.bottom, 12)

            // Latency Spectrum (.spec)
            GeometryReader { geo in
                let w = geo.size.width
                let h = geo.size.height
                let axisY = h - 28
                let sortedIds = g.nodeIds.sorted { store.nodes[$0].sortMs < store.nodes[$1].sortMs }
                let heights: [CGFloat] = [252, 204, 156, 108, 62]
                let top = Self.spectrumTop(g.nodeIds.map { store.nodes[$0] }.filter { $0.known }.map { $0.ms }.max() ?? 0)
                let edge = Int((top * 0.968).rounded())
                let ticks: [Int] = top <= 250 ? [15, 45, 80, 120, 180, 250] : [15, 45, 80, 150, max(180, Int((top * 0.06).rounded()) * 10), Int(top)]

                ZStack(alignment: .topLeading) {
                    // Zones (WARM, MILD, COLD)
                    ForEach([
                        (0.0, "WARM · 暖"),
                        (Double(logX(80, top: top)), "MILD · 温"),
                        (Double(logX(150, top: top)), "COLD · 凉")
                    ], id: \.1) { frac, title in
                        let zx = w * CGFloat(frac)
                        Path { p in
                            p.move(to: CGPoint(x: zx, y: 0))
                            p.addLine(to: CGPoint(x: zx, y: axisY))
                        }
                        .stroke(theme.line, style: StrokeStyle(lineWidth: 1, dash: [4, 4]))

                        Text(title)
                            .font(.system(size: 10.5, design: .monospaced))
                            .tracking(1.6)
                            .foregroundColor(theme.mute)
                            .position(x: zx + 42, y: 8)
                    }

                    // Bottom Axis & Ticks
                    Path { p in
                        p.move(to: CGPoint(x: 0, y: axisY))
                        p.addLine(to: CGPoint(x: w, y: axisY))
                    }
                    .stroke(theme.ink, lineWidth: 1.5)

                    ForEach(ticks, id: \.self) { tick in
                        let tx = w * logX(Double(tick), top: top)
                        Text("\(tick)ms")
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundColor(theme.mute)
                            .position(x: tx, y: h - 10)
                    }

                    // Stems
                    ForEach(Array(sortedIds.enumerated()), id: \.element) { rank, nodeId in
                        let node = store.nodes[nodeId]
                        let isSel = g.selected == nodeId
                        let sx = w * logX(Double(node.known ? min(edge, node.ms) : edge), top: top)
                        let stemH = heights[rank % heights.count]
                        let topY = axisY - stemH
                        let flip = logX(Double(node.known ? node.ms : edge), top: top) > 0.78

                        // Stem line + short horizontal flag tick
                        Path { p in
                            p.move(to: CGPoint(x: sx, y: axisY))
                            p.addLine(to: CGPoint(x: sx, y: topY))
                            p.addLine(to: CGPoint(x: flip ? sx - 6 : sx + 6, y: topY))
                        }
                        .stroke(theme.ink, style: StrokeStyle(lineWidth: isSel ? 2.6 : 1.5, lineJoin: .round, dash: node.verdict.level == 2 ? [4, 3] : []))

                        // Stem base dot
                        Circle()
                            .fill(node.verdict.level == 2 ? theme.bg : theme.ink)
                            .frame(width: 7, height: 7)
                            .overlay(Circle().stroke(theme.ink, lineWidth: 1.5))
                            .position(x: sx, y: axisY)

                        // Stem flag button — anchored at a constant 6pt distance from vertical stem sx
                        Button(action: {
                            store.selectNode(inGroup: store.selectedGroupIdx, nodeId: nodeId)
                        }) {
                            VStack(alignment: flip ? .trailing : .leading, spacing: 1) {
                                Text(node.name)
                                    .font(.system(size: 13, weight: .bold))
                                    .foregroundColor(isSel ? theme.bg : theme.ink)
                                Text((node.known ? "\(node.ms)ms" + (node.samples >= 2 ? " ±\(node.jitter)" : "") : "未测") + (node.samples >= 2 && node.loss > 0 ? " · 丢 \(node.loss)%" : ""))
                                    .font(.system(size: 10.5, design: .monospaced))
                                    .foregroundColor(isSel ? theme.bg.opacity(0.75) : theme.mute)
                            }
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(isSel ? theme.ink : theme.bg)
                            .overlay(
                                RoundedRectangle(cornerRadius: 3)
                                    .stroke(isSel ? theme.ink : theme.line.opacity(0.8), lineWidth: 1)
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 3))
                        }
                        .buttonStyle(.plain)
                        .onHover { inside in
                            store.hoveredNodeId = inside ? nodeId : nil
                        }
                        .frame(width: 170, alignment: flip ? .trailing : .leading)
                        .position(x: flip ? (sx - 91) : (sx + 91), y: topY + 13)
                    }
                }
                .opacity(store.isTestingNodes ? 0.35 : 1.0)
            }
            .frame(height: 316)

            // Bottom Node Stability Bar (#nd)
            HStack(alignment: .bottom, spacing: 18) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(detailNodeId == g.selected ? "当前窝点" : "看看这个")
                        .font(.system(size: 10, design: .monospaced))
                        .tracking(1.5)
                        .foregroundColor(theme.mute)
                    Text(detailNode.name)
                        .font(.system(size: 20, weight: .black))
                        .foregroundColor(theme.ink)
                    Text("\(detailNode.proto) · \(g.name)")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(theme.mute)
                }
                .frame(width: 190, alignment: .leading)

                ndMetric(label: "延迟", val: detailNode.msValue, unit: detailNode.known ? "ms" : "")
                ndMetric(label: "抖动", val: detailNode.jitterText, unit: detailNode.samples >= 2 ? "ms" : "")
                ndMetric(label: "丢包", val: detailNode.lossText, unit: detailNode.samples >= 2 ? "%" : "")
                ndMetric(label: detailNode.history.isEmpty ? "探测成功" : "探测成功 · 近 \(detailNode.history.count) 次", val: detailNode.uptimeText, unit: detailNode.uptime >= 0 ? "%" : "")

                // 18-sample Sparkline
                VStack(alignment: .leading, spacing: 4) {
                    Text(detailNode.history.isEmpty ? "还没测过 · × = 没回应" : "最近 \(detailNode.history.count) 次测速 · × = 没回应")
                        .font(.system(size: 10, design: .monospaced))
                        .tracking(1.4)
                        .foregroundColor(theme.mute)
                    NodeSparklineView(history: detailNode.history, theme: theme)
                        .frame(height: 44)
                }
                .frame(maxWidth: .infinity)

                // Verdict (.vd)
                VStack(alignment: .leading, spacing: 3) {
                    Text(detailNode.verdict.title)
                        .font(.system(size: 15, weight: .black))
                        .foregroundColor(theme.ink)
                    Text(detailNode.verdict.desc)
                        .font(.system(size: 12))
                        .foregroundColor(theme.mute)
                }
                .frame(width: 230, alignment: .leading)
            }
            .padding(.top, 14)
            .overlay(alignment: .top) {
                Rectangle().fill(theme.ink).frame(height: 1.5)
            }
        }
    }

    private func ndMetric(label: String, val: String, unit: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.system(size: 10, design: .monospaced))
                .tracking(1.4)
                .foregroundColor(theme.mute)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
            HStack(alignment: .firstTextBaseline, spacing: 2) {
                Text(val)
                    .font(.system(size: 22, weight: .bold, design: .monospaced))
                    .foregroundColor(theme.ink)
                Text(unit)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(theme.mute)
            }
        }
        .frame(width: 88, alignment: .leading)
    }
}

struct NodeSparklineView: View {
    let history: [Int?]
    let theme: PurrTheme

    var body: some View {
        GeometryReader { g in
            PurrCanvasView(size: g.size) { ctx, size in
                let valid = history.compactMap { $0 }
                guard !valid.isEmpty, history.count > 1 else { return }
                let mx = Double(valid.max() ?? 100) * 1.15
                let mn = Double(valid.min() ?? 10) * 0.85
                let avg = Double(valid.reduce(0, +)) / Double(valid.count)
                func xAt(_ i: Int) -> CGFloat { CGFloat(i) / CGFloat(history.count - 1) * size.width }
                func yAt(_ v: Double) -> CGFloat { size.height - CGFloat((v - mn) / max(1.0, mx - mn)) * (size.height - 8) - 4 }

                var avPath = Path()
                avPath.move(to: CGPoint(x: 0, y: yAt(avg)))
                avPath.addLine(to: CGPoint(x: size.width, y: yAt(avg)))
                ctx.stroke(avPath, with: .color(theme.line), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))

                var line = Path()
                var drawing = false
                for (idx, sample) in history.enumerated() {
                    let x = xAt(idx)
                    if let val = sample {
                        let pt = CGPoint(x: x, y: yAt(Double(val)))
                        if drawing { line.addLine(to: pt) }
                        else { line.move(to: pt); drawing = true }
                    } else {
                        drawing = false
                        var xp = Path()
                        xp.move(to: CGPoint(x: x - 3, y: size.height - 7))
                        xp.addLine(to: CGPoint(x: x + 3, y: size.height - 1))
                        xp.move(to: CGPoint(x: x + 3, y: size.height - 7))
                        xp.addLine(to: CGPoint(x: x - 3, y: size.height - 1))
                        ctx.stroke(xp, with: .color(theme.ink), lineWidth: 1.4)
                    }
                }
                ctx.stroke(line, with: .color(theme.ink), style: StrokeStyle(lineWidth: 1.6, lineJoin: .round))
            }
        }
    }
}

// MARK: - Page 2: Subscriptions (#p-sb in PURR-desktop-v5.html)
struct SubsPageView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .firstTextBaseline) {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("订阅").font(.system(size: 34, weight: .black)).foregroundColor(theme.ink)
                    Text("毛线团").font(.system(size: 13, weight: .medium)).foregroundColor(theme.mute)
                }
                Spacer()
                HStack(spacing: 12) {
                    TextField("粘贴订阅链接 https://…", text: $store.subInputUrl)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12, design: .monospaced))
                        .frame(width: 340)
                        .padding(.vertical, 5)
                        .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }

                    PurrBracketButton(icon: "imp", label: store.isImporting ? "闻一闻…" : "导入", theme: theme) {
                        store.performImport(overrideUrl: store.subInputUrl)
                    }
                }
            }

            if store.subscriptions.isEmpty {
                SubsEmptyGuideView(store: store, theme: theme)
            }

            VStack(spacing: 0) {
                ForEach(store.subscriptions) { sub in
                    HStack(spacing: 32) {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(sub.name)
                                .font(.system(size: 30, weight: .black))
                                .foregroundColor(theme.ink)
                            Text(sub.desc)
                                .font(.system(size: 11.5, design: .monospaced))
                                .foregroundColor(theme.mute)
                        }
                        .frame(width: 300, alignment: .leading)

                        VStack(alignment: .leading, spacing: 12) {
                            HStack(alignment: .firstTextBaseline, spacing: 6) {
                                Text(sub.usedText)
                                    .font(.system(size: 40, weight: .bold, design: .monospaced))
                                    .foregroundColor(theme.ink)
                                Text("/ \(sub.totalText) GB")
                                    .font(.system(size: 14, weight: .medium, design: .monospaced))
                                    .foregroundColor(theme.mute)
                            }
                            GeometryReader { g in
                                ZStack(alignment: .leading) {
                                    Rectangle().stroke(theme.ink, lineWidth: 1.5)
                                    Rectangle()
                                        .fill(theme.ink)
                                        .frame(width: g.size.width * CGFloat(sub.fraction))
                                }
                            }
                            .frame(height: 10)
                        }
                        .frame(maxWidth: .infinity)

                        VStack(alignment: .trailing, spacing: 6) {
                            if sub.active {
                                Text(store.errKind == "sub" ? "已过期" : "使用中")
                                    .font(.system(size: 11, design: .monospaced))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(theme.ink)
                                    .foregroundColor(theme.bg)
                            }
                            Text("到期 \(sub.exp)")
                                .font(.system(size: 11.5, design: .monospaced))
                                .foregroundColor(theme.mute)
                            HStack(spacing: 12) {
                                if !sub.active {
                                    PurrBracketButton(icon: nil, label: "使用", theme: theme) {
                                        store.activateSubscription(id: sub.id)
                                    }
                                }
                                PurrBracketButton(icon: "upd", label: store.isImporting ? "更新中…" : "更新", theme: theme) {
                                    store.refreshSubscription(id: sub.id)
                                }
                            }
                        }
                        .frame(width: 200, alignment: .trailing)
                    }
                    .padding(.vertical, 24)
                    .overlay(alignment: .top) { Rectangle().fill(theme.ink).frame(height: 1.5) }
                }
                Rectangle().fill(theme.ink).frame(height: 1.5)
            }
        }
    }
}

// MARK: - Page 3: Connections & Logs (#p-cn in PURR-desktop-v5.html)
struct ConnectionsPageView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline, spacing: 24) {
                Text("连接").font(.system(size: 34, weight: .black)).foregroundColor(theme.ink)

                HStack(spacing: 2) {
                    ForEach([("live", "实时"), ("log", "爪印 · 日志")], id: \.0) { k, title in
                        let active = store.connPane == k
                        Button(action: { store.connPane = k }) {
                            Text(title)
                                .font(.system(size: 12, design: .monospaced))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(active ? theme.ink : Color.clear)
                                .foregroundColor(active ? theme.bg : theme.mute)
                                .clipShape(RoundedRectangle(cornerRadius: 3))
                        }
                        .buttonStyle(.plain)
                    }
                }

                Spacer()

                if store.connPane == "live" {
                    HStack(spacing: 16) {
                        TextField("搜索域名 / 应用 / 规则 / 窝点", text: $store.connQuery)
                            .textFieldStyle(.plain)
                            .font(.system(size: 12, design: .monospaced))
                            .frame(width: 260)
                            .padding(.vertical, 4)
                            .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }

                        PurrBracketButton(icon: "cut", label: "全部断开", theme: theme) {
                            store.closeAllConnections()
                        }
                    }
                } else {
                    HStack(spacing: 16) {
                        HStack(spacing: 2) {
                            ForEach([("all", "全部"), ("purr", "呼噜"), ("warn", "警告")], id: \.0) { lv, title in
                                let active = store.logLevelFilter == lv
                                Button(action: { store.logLevelFilter = lv }) {
                                    Text(title)
                                        .font(.system(size: 12, design: .monospaced))
                                        .padding(.horizontal, 8)
                                        .padding(.vertical, 3)
                                        .background(active ? theme.ink : Color.clear)
                                        .foregroundColor(active ? theme.bg : theme.mute)
                                        .clipShape(RoundedRectangle(cornerRadius: 3))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        PurrBracketButton(icon: nil, label: "清空", theme: theme) {
                            store.logs.removeAll()
                        }
                    }
                }
            }

            if store.connPane == "live" {
                liveTable
            } else {
                logTerminal
            }
        }
    }

    private var liveTable: some View {
        let q = store.connQuery.lowercased()
        let rows = store.conns.filter { c in
            q.isEmpty || (c.host + c.app + c.chain + c.rule).lowercased().contains(q)
        }
        return VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("HOST").frame(width: 240, alignment: .leading)
                Text("APP").frame(width: 110, alignment: .leading)
                Text("RULE").frame(width: 170, alignment: .leading)
                Text("CHAIN").frame(width: 220, alignment: .leading)
                Spacer()
                Text("DOWN").frame(width: 90, alignment: .trailing)
                Text("UP").frame(width: 90, alignment: .trailing)
                Text("AGE").frame(width: 70, alignment: .trailing)
                Text("").frame(width: 36)
            }
            .font(.system(size: 10.5, weight: .medium, design: .monospaced))
            .tracking(1.5)
            .foregroundColor(theme.mute)
            .padding(.bottom, 8)
            .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }

            if rows.isEmpty {
                HStack(spacing: 10) {
                    PurrIconView(name: store.isAwake ? "cn" : "ov", size: 24, isAwake: store.isAwake, color: theme.mute)
                    Text(store.isAwake ? "没有匹配的连接" : "zzz… 它睡着了，没有连接。")
                        .font(.system(size: 14))
                        .foregroundColor(theme.mute)
                }
                .padding(.top, 36)
            } else {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(rows.prefix(13)) { c in
                            HStack {
                                Text(c.host)
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundColor(theme.ink)
                                    .frame(width: 240, alignment: .leading)
                                    .lineLimit(1)
                                Text(c.app)
                                    .foregroundColor(theme.ink)
                                    .frame(width: 110, alignment: .leading)
                                Text(c.rule)
                                    .foregroundColor(theme.mute)
                                    .frame(width: 170, alignment: .leading)
                                // Shows the route the core really used; changing rules is not offered here.
                                policyBadge(pol: c.pol, chain: c.chain)
                                .frame(width: 220, alignment: .leading)
                                Spacer()
                                Text(fmtKB(c.dn)).frame(width: 90, alignment: .trailing)
                                Text(fmtKB(c.up)).frame(width: 90, alignment: .trailing)
                                Text(c.ageLabel).frame(width: 70, alignment: .trailing)
                                Button(action: {
                                    store.closeConnection(id: c.id)
                                }) {
                                    PurrIconView(name: "cut", size: 13, strokeWidth: 2, color: theme.ink)
                                }
                                .buttonStyle(.plain)
                                .frame(width: 36, alignment: .trailing)
                            }
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundColor(theme.ink)
                            .padding(.vertical, 8)
                            .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func policyBadge(pol: String, chain: String) -> some View {
        if pol == "DIRECT" {
            HStack(spacing: 4) {
                PurrIconView(name: "direct", size: 12, strokeWidth: 2, color: theme.ink)
                Text("直连")
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .overlay(Rectangle().stroke(theme.ink, lineWidth: 1))
        } else if pol == "REJECT" {
            HStack(spacing: 4) {
                PurrIconView(name: "mouse", size: 12, strokeWidth: 2, color: theme.ink)
                Text("抓老鼠")
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .overlay(Rectangle().stroke(theme.ink, style: StrokeStyle(lineWidth: 1, dash: [3, 2])))
        } else {
            Text(chain.isEmpty ? pol : chain)
                .padding(.horizontal, 6)
                .padding(.vertical, 1)
                .background(theme.ink)
                .foregroundColor(theme.bg)
        }
    }

    private func fmtKB(_ v: Double) -> String {
        v > 1024 ? String(format: "%.1f MB", v / 1024.0) : "\(Int(round(v))) KB"
    }

    private var logTerminal: some View {
        let filtered = store.logs.filter { store.logLevelFilter == "all" || $0.level == store.logLevelFilter }
        return VStack(alignment: .leading, spacing: 6) {
            ForEach(filtered.prefix(18).reversed()) { item in
                HStack(spacing: 0) {
                    Text(item.time)
                        .foregroundColor(theme.mute)
                        .frame(width: 86, alignment: .leading)
                    Text(item.level.uppercased())
                        .fontWeight(item.level == "purr" ? .heavy : .regular)
                        .underline(item.level == "warn")
                        .foregroundColor(item.level == "info" ? theme.mute : theme.ink)
                        .frame(width: 64, alignment: .leading)
                    Text(item.msg)
                        .foregroundColor(theme.ink)
                }
                .font(.system(size: 12.5, design: .monospaced))
            }
        }
        .padding(.top, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .top) { Rectangle().fill(theme.ink).frame(height: 1.5) }
    }
}

// MARK: - Page 4: Rules (#p-rl in PURR-desktop-v5.html)
struct RulesPageView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    var body: some View {
        let q = store.ruleQuery.lowercased()
        let filtered = store.allRules.filter { r in
            q.isEmpty || "\(r.kind) \(r.match) \(r.policy)".lowercased().contains(q)
        }
        // The core reports per-rule match counts (`extra.hitCount`); without them the column hides.
        let hasHits = store.allRules.contains { $0.hits != nil }
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("规则").font(.system(size: 34, weight: .black)).foregroundColor(theme.ink)
                    Text("猫的规矩").font(.system(size: 13, weight: .medium)).foregroundColor(theme.mute)
                }
                Spacer()
                HStack(spacing: 16) {
                    TextField("搜索规则", text: $store.ruleQuery)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12, design: .monospaced))
                        .frame(width: 260)
                        .padding(.vertical, 4)
                        .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }
                    Text("\(store.allRules.count) 条 · 显示 \(filtered.count)")
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundColor(theme.mute)
                }
            }

            HStack {
                Text("#").frame(width: 50, alignment: .leading)
                Text("TYPE").frame(width: 190, alignment: .leading)
                Text("MATCH").frame(width: 280, alignment: .leading)
                Text("POLICY").frame(width: 180, alignment: .leading)
                Spacer()
                if hasHits {
                    Text("HITS · 启动以来").frame(width: 120, alignment: .trailing)
                }
            }
            .font(.system(size: 10.5, weight: .medium, design: .monospaced))
            .tracking(1.5)
            .foregroundColor(theme.mute)
            .padding(.bottom, 8)
            .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }

            ScrollView {
                VStack(spacing: 0) {
                    ForEach(filtered) { r in
                        HStack {
                            Text(String(format: "%02d", r.id))
                                .foregroundColor(theme.mute)
                                .frame(width: 50, alignment: .leading)
                            Text(r.kind)
                                .foregroundColor(theme.ink)
                                .frame(width: 190, alignment: .leading)
                            Text(r.match)
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundColor(theme.ink)
                                .frame(width: 280, alignment: .leading)
                            Text(r.policy == "DIRECT" ? "直连" : (r.policy == "REJECT" ? "抓老鼠" : r.policy))
                                .padding(.horizontal, 6)
                                .padding(.vertical, 1)
                                .background(r.policy != "DIRECT" && r.policy != "REJECT" ? theme.ink : Color.clear)
                                .foregroundColor(r.policy != "DIRECT" && r.policy != "REJECT" ? theme.bg : theme.ink)
                                .overlay(Rectangle().stroke(theme.ink, lineWidth: 1))
                                .frame(width: 180, alignment: .leading)
                            Spacer()
                            if hasHits {
                                Text(r.hits.map { "\($0)" } ?? "—")
                                    .foregroundColor(theme.ink)
                                    .frame(width: 120, alignment: .trailing)
                            }
                        }
                        .font(.system(size: 12, design: .monospaced))
                        .padding(.vertical, 6)
                        .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }
                    }
                }
            }
            .opacity(store.mode == "rule" ? 1.0 : 0.3)
        }
    }
}

// MARK: - Page 5: Settings (#p-st in PURR-desktop-v5.html)
struct SettingsPageView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme
    @State private var hoveredRow: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .firstTextBaseline) {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text("习惯").font(.system(size: 34, weight: .black)).foregroundColor(theme.ink)
                    Text("它的小脾气").font(.system(size: 13, weight: .medium)).foregroundColor(theme.mute)
                }
                Spacer()
                HStack(spacing: 14) {
                    PurrBracketButton(icon: "", label: "复制终端命令", theme: theme) {
                        store.copyTerminalProxyCommand()
                    }
                    PurrBracketButton(icon: "", label: "刷新 DNS", theme: theme) {
                        store.flushDnsCache()
                    }
                    PurrBracketButton(icon: "cut", label: "退出应用", theme: theme) {
                        NSApplication.shared.terminate(nil)
                    }
                }
            }

            HStack(alignment: .top, spacing: 56) {
                // Column 1: HABITS · 习惯
                VStack(alignment: .leading, spacing: 0) {
                    sectionHeader("HABITS · 习惯")
                    switchRow(icon: "wake", title: "醒来就呼噜", desc: "开机启动，并自动连接", isOn: $store.autoWake)
                    switchRow(icon: "swap", title: "换个窝继续睡", desc: "窝点断线时，自动换到最暖的窝", isOn: $store.autoSwap)
                    switchRow(icon: "mouse", title: "抓老鼠", desc: "拦截广告与追踪器", isOn: $store.blockAds)
                    segmentedRow(
                        icon: "cut",
                        title: "点关闭按钮时",
                        desc: "点窗口关闭按钮 (×) 是退到后台托盘，还是直接退出应用",
                        options: [("tray", "后台托盘"), ("quit", "直接退出")],
                        selection: $store.closeAction
                    )
                    segmentedRow(
                        icon: "snd",
                        title: "呼噜声",
                        desc: "连上 / 断开时呼噜一声，或者一直呼噜",
                        options: [("off", "静音"), ("ping", "一声"), ("loop", "一直")],
                        selection: $store.soundMode
                    )
                }
                .frame(maxWidth: .infinity)

                // Column 2: NETWORK · 网络
                VStack(alignment: .leading, spacing: 0) {
                    sectionHeader("NETWORK · 网络")
                    HStack(spacing: 14) {
                        PurrIconView(name: "port", size: 22, color: theme.ink)
                            .frame(width: 28)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("混合端口").font(.system(size: 15, weight: .bold)).foregroundColor(theme.ink)
                            Text("HTTP / SOCKS5 共用一个").font(.system(size: 12)).foregroundColor(theme.mute)
                        }
                        Spacer()
                        TextField("7890", text: $store.mixedPort)
                            .textFieldStyle(.plain)
                            .font(.system(size: 16, weight: .bold, design: .monospaced))
                            .multilineTextAlignment(.trailing)
                            .frame(width: 76)
                            .padding(.vertical, 4)
                            .overlay(alignment: .bottom) {
                                Rectangle().fill(theme.ink).frame(height: 1.5)
                            }
                    }
                    .frame(minHeight: 72)
                    .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }

                    switchRow(
                        icon: "lan",
                        title: "分享猫窝",
                        desc: store.allowLan ? "局域网设备可连 \(PurrStore.lanIPv4() ?? "本机局域网 IP"):\(store.mixedPort)" : "允许局域网设备连接",
                        isOn: $store.allowLan
                    )
                    switchRow(icon: "v6", title: "IPv6", desc: "启用 IPv6 代理", isOn: $store.ipv6)
                    segmentedRow(
                        icon: "dns",
                        title: "DNS",
                        desc: "加密 DNS（DoH）",
                        options: [("1.1.1.1", "1.1.1.1"), ("8.8.8.8", "8.8.8.8"), ("223.5.5.5", "223.5.5.5")],
                        selection: $store.dnsServer
                    )

                    sectionHeader("ABOUT · 关于与版本")
                        .padding(.top, 16)
                    HStack(spacing: 14) {
                        PurrIconView(name: "ov", size: 22, isAwake: store.isAwake, color: theme.ink)
                            .frame(width: 28)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("PURR v\(PurrStore.appVersion)").font(.system(size: 15, weight: .bold)).foregroundColor(theme.ink)
                            Text("mihomo 内核 · 原生桌面代理").font(.system(size: 12)).foregroundColor(theme.mute)
                        }
                        Spacer()
                        PurrBracketButton(icon: "upd", label: "检查更新", theme: theme) {
                            store.checkForUpdates()
                        }
                    }
                    .frame(minHeight: 68)
                    .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }
                }
                .frame(maxWidth: .infinity)
            }
        }
    }

    private func sectionHeader(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10.5, weight: .medium, design: .monospaced))
            .tracking(1.6)
            .foregroundColor(theme.mute)
            .padding(.bottom, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }
    }

    private func switchRow(icon: String, title: String, desc: String, isOn: Binding<Bool>) -> some View {
        let hov = hoveredRow == title || PurrSnapshot.forced("row:\(title)")
        return Button(action: {
            isOn.wrappedValue.toggle()
            store.showToast(title + (isOn.wrappedValue ? " 已开启" : " 已关闭"))
        }) {
            HStack(spacing: 14) {
                PurrIconView(name: icon, size: 22, isAwake: isOn.wrappedValue, color: theme.ink, isHovered: false, allowSelfHover: false)
                    .frame(width: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 15, weight: .bold)).foregroundColor(theme.ink)
                    Text(desc).font(.system(size: 12)).foregroundColor(theme.mute)
                }
                Spacer()
                CatSwitchView(isOn: isOn.wrappedValue, ink: theme.ink, bg: theme.bg, mute: theme.mute, line: theme.line, externalHover: hov)
            }
            .frame(minHeight: 72)
            .contentShape(Rectangle())
            .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }
        }
        .buttonStyle(.plain)
        .onHover { inside in hoveredRow = inside ? title : (hoveredRow == title ? nil : hoveredRow) }
    }

    private func segmentedRow(icon: String, title: String, desc: String, options: [(String, String)], selection: Binding<String>) -> some View {
        return HStack(spacing: 14) {
            PurrIconView(name: icon, size: 22, isAwake: store.isAwake, color: theme.ink, isHovered: false, allowSelfHover: false)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 15, weight: .bold)).foregroundColor(theme.ink)
                Text(desc).font(.system(size: 12)).foregroundColor(theme.mute)
            }
            Spacer()
            HStack(spacing: 0) {
                ForEach(options, id: \.0) { k, label in
                    let active = selection.wrappedValue == k
                    Button(action: {
                        selection.wrappedValue = k
                        store.showToast("\(title) → \(label)")
                    }) {
                        Text(label)
                            .font(.system(size: 12, weight: active ? .bold : .regular, design: .monospaced))
                            .padding(.horizontal, 11)
                            .frame(height: 30)
                            .background(active ? theme.ink : Color.clear)
                            .foregroundColor(active ? theme.bg : theme.ink)
                    }
                    .buttonStyle(.plain)
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(theme.ink, lineWidth: 1.5))
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        .frame(minHeight: 72)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.line).frame(height: 1) }
        .onHover { inside in hoveredRow = inside ? title : (hoveredRow == title ? nil : hoveredRow) }
    }
}


// MARK: - Empty subscriptions: import guide (same layout as the Windows page)
struct SubsEmptyGuideView: View {
    @ObservedObject var store: PurrStore
    let theme: PurrTheme

    var body: some View {
        VStack(spacing: 10) {
            PurrIconView(name: "sb", size: 44, strokeWidth: 1.6, isAwake: false, color: theme.mute)
                .padding(.bottom, 6)
            Text("还没有毛线团")
                .font(.system(size: 24, weight: .black))
                .foregroundColor(theme.ink)
            HStack(spacing: 8) {
                Text("粘贴订阅链接").fontWeight(.bold)
                Text("→")
                Text("导入").fontWeight(.bold)
            }
            .font(.system(size: 14))
            .foregroundColor(theme.ink)
            Text("在右上角或下方输入机场提供的订阅地址（Clash / 通用订阅），即可读取全部窝点。")
                .font(.system(size: 12))
                .foregroundColor(theme.mute)
            HStack(spacing: 12) {
                TextField("粘贴订阅链接 https://…", text: $store.firstRunUrl)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12, design: .monospaced))
                    .frame(width: 440)
                    .padding(.vertical, 5)
                    .overlay(alignment: .bottom) { Rectangle().fill(theme.ink).frame(height: 1.5) }
                Button(action: { store.performImport(overrideUrl: store.firstRunUrl) }) {
                    HStack(spacing: 6) {
                        PurrIconView(name: "imp", size: 14, strokeWidth: 1.9, isAwake: true, color: theme.bg)
                        Text(store.isImporting ? "闻一闻…" : "导入")
                            .font(.system(size: 12.5, weight: .bold))
                    }
                    .foregroundColor(theme.bg)
                    .padding(.horizontal, 16)
                    .frame(height: 32)
                    .background(theme.ink)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                }
                .buttonStyle(.plain)
            }
            .padding(.top, 14)
            HStack(spacing: 6) {
                Text("也可以").foregroundColor(theme.mute)
                Button(action: {
                    let text = NSPasteboard.general.string(forType: .string)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    if text.hasPrefix("http://") || text.hasPrefix("https://") {
                        store.firstRunUrl = text
                        store.performImport(overrideUrl: text)
                    } else {
                        store.showToast("剪贴板里没有订阅链接")
                    }
                }) {
                    Text("从剪贴板粘贴并导入").fontWeight(.bold).foregroundColor(theme.ink).underline()
                }
                .buttonStyle(.plain)
            }
            .font(.system(size: 12))
            .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 56)
    }
}


/// Import nudge: |sin(pi*phase)| bumps; animating phase by 3 gives three left pushes that end at 0.
struct PurrShakeEffect: GeometryEffect {
    var phase: Double
    init(phase: Int) { self.phase = Double(phase) }
    var animatableData: Double {
        get { phase }
        set { phase = newValue }
    }
    func effectValue(size: CGSize) -> ProjectionTransform {
        ProjectionTransform(CGAffineTransform(translationX: -6 * abs(sin(phase * .pi)), y: 0))
    }
}
