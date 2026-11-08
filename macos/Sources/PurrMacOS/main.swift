import SwiftUI
import AppKit
import Darwin
import PurrCore

// MARK: - Main Content View (#app in PURR-desktop-v5.html)
struct PurrMainView: View {
    @ObservedObject var store: PurrStore
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let isDark = colorScheme == .dark
        let sleepTheme = PurrTheme.current(isAwake: false, mode: store.mode, hasErr: store.errKind != nil, isDark: isDark)
        let activeTheme = PurrTheme.current(isAwake: store.isAwake, mode: store.mode, hasErr: store.errKind != nil, isDark: isDark)

        ZStack(alignment: .topTrailing) {
            if store.floodAnimating {
                sleepTheme.bg.ignoresSafeArea()
                PurrCanvasView(size: CGSize(width: 1200, height: 760)) { ctx, size in
                    let maxR = hypot(size.width, size.height) * 1.15
                    let r = maxR * store.floodRadius
                    let rect = CGRect(x: store.floodX - r, y: store.floodY - r, width: r * 2, height: r * 2)
                    ctx.fill(Path(ellipseIn: rect), with: .color(activeTheme.warm))
                }
                .ignoresSafeArea()
                .allowsHitTesting(false)
            } else {
                (store.isAwake ? activeTheme.warm : sleepTheme.bg)
                    .ignoresSafeArea()
            }

            // Main 3-row layout: 48px TopBar + 136px EarBand + 1fr Pages
            VStack(spacing: 0) {
                PurrTopBarView(store: store, theme: activeTheme)
                PurrEarBandView(store: store, theme: activeTheme)

                ZStack {
                    switch store.page {
                    case 0:
                        OverviewPageView(store: store, theme: activeTheme)
                    case 1:
                        NodesPageView(store: store, theme: activeTheme)
                    case 2:
                        SubsPageView(store: store, theme: activeTheme)
                    case 3:
                        ConnectionsPageView(store: store, theme: activeTheme)
                    case 4:
                        RulesPageView(store: store, theme: activeTheme)
                    default:
                        SettingsPageView(store: store, theme: activeTheme)
                    }
                }
                .padding(.horizontal, 40)
                .padding(.vertical, 28)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            // 保险：页面内容万一超高，只往下溢出被裁掉，顶栏（红绿灯 + 状态徽章）始终贴窗口顶
            .frame(width: 1200, height: 760, alignment: .top)
            .clipped()

            // Full-window Service Status & Exit IP Diagnostics Modal (covers TopBar + EarBand + Pages)
            if store.showServiceDetail {
                ServiceDetailModalView(store: store, theme: activeTheme)
                    .frame(width: 1200, height: 760)
                    .zIndex(50)
            }

            // Toast notification (.toast)
            if store.toastVisible {
                Text(store.toastText)
                    .font(.system(size: 12, design: .monospaced))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 5)
                    .background(activeTheme.ink)
                    .foregroundColor(activeTheme.bg)
                    .padding(.top, 58)
                    .padding(.trailing, 20)
                    .transition(.opacity.combined(with: .offset(y: -4)))
                    .zIndex(60)
            }
        }
        .frame(width: 1200, height: 760)
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color.black.opacity(0.14), lineWidth: 1)
        )
        // 屏幕太矮、窗口被收到可见区域以内时：内容顶对齐，只裁底部
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

// MARK: - Tray Popover View (.tray in PURR-desktop-v5.html)
struct PurrTrayPopoverView: View {
    @ObservedObject var store: PurrStore
    let onOpenMain: () -> Void
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let theme = PurrTheme.current(isAwake: store.isAwake, mode: store.mode, hasErr: store.errKind != nil, isDark: colorScheme == .dark)
        let n = store.currentPrimaryNode
        let sortedTop5 = Array(store.groups[0].nodeIds.sorted { store.nodes[$0].sortMs < store.nodes[$1].sortMs }.prefix(5))

        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(store.firstRun ? "没有窝" : (store.errKind != nil ? "卡住了" : (store.isAwake ? (store.mode == "direct" ? "醒着 · 直连" : "呼噜中") : "睡着了")))
                        .font(.system(size: 15, weight: .heavy))
                        .foregroundColor(theme.ink)
                    Text(store.firstRun ? "先导入订阅" : (store.errKind != nil ? store.troubleInfo(for: store.errKind!).st : (store.isAwake ? (store.mode == "direct" ? "不经过窝点 · \(store.exitInfo().ip)" : "\(n.name) · \(n.msText) · \(store.exitInfo().ip)") : "系统代理已关 · 端口 \(store.mixedPort) 仍在")))
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundColor(theme.mute)
                }
                Spacer()
                Button(action: {
                    if store.firstRun {
                        store.showToast("先导入订阅")
                        return
                    }
                    if store.errKind != nil { store.setErr(nil) }
                    if store.isAwake { store.sleep() } else { store.wake() }
                }) {
                    CatSwitchView(isOn: store.isAwake, ink: theme.ink, bg: theme.bg, mute: theme.mute, line: theme.line)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 13)

            VStack(alignment: .leading, spacing: 6) {
                Text("走法").font(.system(size: 10, design: .monospaced)).tracking(1.5).foregroundColor(theme.mute)
                HStack(spacing: 0) {
                    ForEach([("rule", "分流"), ("global", "全局"), ("direct", "直连")], id: \.0) { k, title in
                        let active = store.mode == k
                        Button(action: { store.switchMode(k) }) {
                            Text(title)
                                .font(.system(size: 12, weight: active ? .bold : .regular, design: .monospaced))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 5)
                                .background(active ? theme.ink : Color.clear)
                                .foregroundColor(active ? theme.bg : theme.ink)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .overlay(RoundedRectangle(cornerRadius: 4).stroke(theme.ink, lineWidth: 1.5))
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .overlay(alignment: .top) { Rectangle().fill(theme.line).frame(height: 1) }

            VStack(alignment: .leading, spacing: 6) {
                Text("接管").font(.system(size: 10, design: .monospaced)).tracking(1.5).foregroundColor(theme.mute)
                HStack(spacing: 8) {
                    trayCapButton(icon: "sys", label: "系统代理", on: store.sysProxy, dim: false, theme: theme) {
                        store.sysProxy.toggle()
                        store.showToast("系统代理" + (store.sysProxy ? " 已开启" : " 已关闭"))
                    }
                    trayCapButton(icon: "tun", label: (store.tunAuthorized || store.tunEnabled) ? "虚拟网卡" : "虚拟网卡 · 需授权", on: store.tunEnabled, dim: false, theme: theme) {
                        store.toggleTun()
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .overlay(alignment: .top) { Rectangle().fill(theme.line).frame(height: 1) }

            VStack(alignment: .leading, spacing: 4) {
                Text("节点选择").font(.system(size: 10, design: .monospaced)).tracking(1.5).foregroundColor(theme.mute)
                ForEach(sortedTop5, id: \.self) { i in
                    let nd = store.nodes[i]
                    let sel = store.groups[0].selected == i
                    Button(action: {
                        store.selectNode(inGroup: 0, nodeId: i)
                    }) {
                        HStack(spacing: 8) {
                            Circle()
                                .fill(sel ? theme.ink : Color.clear)
                                .frame(width: 7, height: 7)
                                .overlay(Circle().stroke(theme.ink, lineWidth: 1.5))
                            Text(nd.name)
                                .font(.system(size: 12, weight: sel ? .bold : .regular, design: .monospaced))
                                .foregroundColor(theme.ink)
                            Spacer()
                            Text(nd.msText)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(theme.mute)
                        }
                        .padding(.vertical, 3)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .overlay(alignment: .top) { Rectangle().fill(theme.line).frame(height: 1) }

            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(String(format: "↑ %.1f MB/s", store.upSpeed))
                    Text(String(format: "↓ %.1f MB/s", store.dnSpeed))
                        .fontWeight(.bold)
                        .foregroundColor(theme.ink)
                }
                Spacer()
                Text(store.memMB > 0 ? String(format: "内存 %.0f MB", store.memMB) : "内存 —")
                Spacer()
                Text(String(format: "今日 %.2f GB", (store.totalProxyMB + store.totalDirectMB) / 1024.0))
            }
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundColor(theme.mute)
            .padding(.horizontal, 16)
            .padding(.vertical, 9)
            .overlay(alignment: .top) { Rectangle().fill(theme.line).frame(height: 1) }

            VStack(spacing: 2) {
                Button(action: { store.copyTerminalProxyCommand() }) {
                    HStack {
                        Text("复制终端代理命令")
                        Spacer()
                        Text("⌘C").font(.system(size: 11, design: .monospaced)).opacity(0.6)
                    }
                    .font(.system(size: 12.5))
                    .foregroundColor(theme.ink)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                }
                .buttonStyle(.plain)

                Button(action: onOpenMain) {
                    HStack {
                        Text("打开 PURR")
                        Spacer()
                        Text("⌘O").font(.system(size: 11, design: .monospaced)).opacity(0.6)
                    }
                    .font(.system(size: 12.5))
                    .foregroundColor(theme.ink)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                }
                .buttonStyle(.plain)

                Button(action: { NSApp.terminate(nil) }) {
                    HStack {
                        Text("退出")
                        Spacer()
                        Text("⌘Q").font(.system(size: 11, design: .monospaced)).opacity(0.6)
                    }
                    .font(.system(size: 12.5))
                    .foregroundColor(theme.ink)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                }
                .buttonStyle(.plain)
            }
            .padding(8)
            .overlay(alignment: .top) { Rectangle().fill(theme.line).frame(height: 1) }
        }
        .frame(width: 308)
        .background(theme.bg)
    }

    private func trayCapButton(icon: String, label: String, on: Bool, dim: Bool, theme: PurrTheme, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 5) {
                PurrIconView(name: icon, size: 14, strokeWidth: 1.9, isAwake: on, color: on ? theme.bg : theme.ink, isHovered: false)
                Text(label)
                    .font(.system(size: 11.5, weight: on ? .bold : .regular, design: .monospaced))
                    .foregroundColor(on ? theme.bg : theme.ink)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 28)
            .background(on ? theme.ink : Color.clear)
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(theme.ink, lineWidth: 1.5))
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .opacity(dim ? 0.6 : 1.0)
        }
        .buttonStyle(.plain)
        .help(dim ? PurrStore.tunUnauthorizedText : label)
    }
}

// MARK: - AppDelegate (Custom Cat-Ear StatusBar Logo + Low-Memory Window Suspension)
final class PurrAppDelegate: NSObject, NSApplicationDelegate {
    let store = PurrStore()
    var window: NSWindow!
    var statusItem: NSStatusItem!
    var popover: NSPopover!
    private var popoverOutsideMonitor: Any?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if PurrSnapshot.active {
            NSApp.setActivationPolicy(.accessory)
            PurrSnapshot.run(store: store)
            return
        }
        NSApp.setActivationPolicy(.regular)
        if let icnsPath = Bundle.main.path(forResource: "AppIcon", ofType: "icns"),
           let iconImg = NSImage(contentsOfFile: icnsPath) {
            NSApp.applicationIconImage = iconImg
        } else if let fallbackImg = NSImage(contentsOfFile: "/Applications/Purr.app/Contents/Resources/AppIcon.icns") {
            NSApp.applicationIconImage = fallbackImg
        }

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 760),
            styleMask: [.borderless, .fullSizeContentView, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.isMovableByWindowBackground = true
        window.level = .normal
        mountMainContentView()
        window.center()
        clampWindowToVisibleScreen()
        window.orderFrontRegardless()

        store.onTogglePin = { [weak self] pinned in
            self?.window.level = pinned ? .floating : .normal
        }
        store.onSuspendWindow = { [weak self] in
            guard let self = self else { return }
            if self.store.closeAction == "quit" {
                NSApplication.shared.terminate(nil)
            } else {
                self.suspendWindowToTray()
            }
        }
        store.onTrayUpdate = { [weak self] in
            self?.updateStatusItemAppearance()
        }

        setupTray()
        setupKeyboardMonitors()

        // 无边框窗口系统不会自动限位：拖动结束 / 换屏 / 改分辨率后都把窗口收回可见区域
        NotificationCenter.default.addObserver(
            forName: NSWindow.didMoveNotification,
            object: window,
            queue: .main
        ) { [weak self] _ in
            // 拖动时只拦顶部（不让标题栏钻进菜单栏/刘海），左右下随用户拖
            self?.clampWindowToVisibleScreen(topOnly: true)
        }
        NotificationCenter.default.addObserver(
            forName: NSWindow.didChangeScreenNotification,
            object: window,
            queue: .main
        ) { [weak self] _ in
            self?.clampWindowToVisibleScreen()
        }
        NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.clampWindowToVisibleScreen()
        }

        NotificationCenter.default.addObserver(
            forName: NSWindow.didChangeOcclusionStateNotification,
            object: window,
            queue: .main
        ) { [weak self] _ in
            guard let self = self else { return }
            let visible = self.window.occlusionState.contains(.visible)
            if PurrVisibility.shared.mainWindowVisible != visible {
                PurrVisibility.shared.mainWindowVisible = visible
            }
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        if PurrSnapshot.active { return }
        purr_core_shutdown()
    }

    private func mountMainContentView() {
        if window.contentView is NSHostingView<PurrMainView> { return }
        let hostingView = NSHostingView(rootView: PurrMainView(store: store))
        // 窗口尺寸由 clampWindowToVisibleScreen 决定，不让 SwiftUI 内容把窗口撑高
        hostingView.sizingOptions = []
        hostingView.wantsLayer = true
        hostingView.layer?.cornerRadius = 12
        hostingView.layer?.cornerCurve = .continuous
        hostingView.layer?.masksToBounds = true
        window.contentView = hostingView
    }

    /// Releases the 1200x760 window backing buffers so standby memory drops to ~20-30 MB
    private func suspendWindowToTray() {
        window.orderOut(nil)
        // 关到托盘：停掉概览页常用网站的自动重测（开窗后遮挡通知会再置回可见）
        PurrVisibility.shared.mainWindowVisible = false
        window.contentView = NSView(frame: .zero)
        malloc_zone_pressure_relief(nil, 0)
    }

    /// 把窗口 frame 限制在当前屏幕的 visibleFrame 内（不进菜单栏/刘海、不压 Dock）。
    /// 屏幕放不下 1200x760 时把窗口缩到可见区域（内容顶对齐，底部被裁）。
    private func clampWindowToVisibleScreen(topOnly: Bool = false) {
        guard let window = window,
              let screen = window.screen ?? NSScreen.main ?? NSScreen.screens.first else { return }
        let vf = screen.visibleFrame
        var f = window.frame
        if topOnly {
            if f.maxY > vf.maxY {
                f.origin.y = vf.maxY - f.height
                window.setFrameOrigin(f.origin)
            }
            return
        }
        f.size.width = min(1200, vf.width)
        f.size.height = min(760, vf.height)
        if f.maxY > vf.maxY { f.origin.y = vf.maxY - f.height }
        if f.minY < vf.minY { f.origin.y = vf.minY }
        if f.maxX > vf.maxX { f.origin.x = vf.maxX - f.width }
        if f.minX < vf.minX { f.origin.x = vf.minX }
        if f != window.frame { window.setFrame(f, display: true) }
    }

    private func restoreWindowFromTray() {
        mountMainContentView()
        clampWindowToVisibleScreen()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func setupTray() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let btn = statusItem.button {
            btn.target = self
            btn.action = #selector(togglePopover(_:))
        }
        updateStatusItemAppearance()

        popover = NSPopover()
        popover.contentSize = NSSize(width: 308, height: 340)
        popover.behavior = .transient
    }

    /// Renders the 22x22 cat-ear logo + optional single-column 2-line stacked speed (↑ on top, ↓ on bottom)
    private func makeCatEarStatusImage(isAwake: Bool, hasError: Bool, upSpeed: Double, dnSpeed: Double) -> NSImage {
        let showSpeed = isAwake && !hasError
        let totalWidth: CGFloat = showSpeed ? 70 : 22
        let img = NSImage(size: NSSize(width: totalWidth, height: 22), flipped: true) { _ in
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

            // Single-column 2-line stacked speed (Up ↑ on top, Down ↓ on bottom)
            if showSpeed {
                let upAttrs: [NSAttributedString.Key: Any] = [
                    .font: NSFont.monospacedSystemFont(ofSize: 8.5, weight: .medium),
                    .foregroundColor: NSColor.black.withAlphaComponent(0.82)
                ]
                let dnAttrs: [NSAttributedString.Key: Any] = [
                    .font: NSFont.monospacedSystemFont(ofSize: 8.5, weight: .bold),
                    .foregroundColor: NSColor.black
                ]
                let upStr = String(format: "↑%4.1fM", upSpeed) as NSString
                let dnStr = String(format: "↓%4.1fM", dnSpeed) as NSString
                upStr.draw(at: NSPoint(x: 24.0, y: 1.0), withAttributes: upAttrs)
                dnStr.draw(at: NSPoint(x: 24.0, y: 10.8), withAttributes: dnAttrs)
            }
            return true
        }
        img.isTemplate = true
        return img
    }

    private func updateStatusItemAppearance() {
        guard let btn = statusItem?.button else { return }
        let awake = store.isAwake && !store.firstRun
        btn.title = ""
        btn.imagePosition = .imageOnly
        btn.image = makeCatEarStatusImage(
            isAwake: awake,
            hasError: store.errKind != nil,
            upSpeed: store.upSpeed,
            dnSpeed: store.dnSpeed
        )
    }

    @objc private func togglePopover(_ sender: AnyObject?) {
        guard let btn = statusItem.button else { return }
        if popover.isShown {
            closeTrayPopover()
        } else {
            let hc = NSHostingController(
                rootView: PurrTrayPopoverView(store: store) { [weak self] in
                    self?.closeTrayPopover()
                    self?.restoreWindowFromTray()
                }
            )
            hc.sizingOptions = .preferredContentSize
            popover.contentViewController = hc
            // 不激活整个 App：激活会把桌面切回 Purr 主窗口所在的空间，全屏应用里就弹不出来
            popover.show(relativeTo: btn.bounds, of: btn, preferredEdge: .minY)
            if let pw = popover.contentViewController?.view.window {
                pw.collectionBehavior.insert([.canJoinAllSpaces, .fullScreenAuxiliary])
                pw.makeKey()
            }
            // 点到别的 App / 桌面 / 菜单栏其他位置时收起（本 App 未激活时 .transient 收不到这些点击）
            if popoverOutsideMonitor == nil {
                popoverOutsideMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]) { [weak self] _ in
                    self?.closeTrayPopover()
                }
            }
        }
    }

    private func closeTrayPopover() {
        if let m = popoverOutsideMonitor {
            NSEvent.removeMonitor(m)
            popoverOutsideMonitor = nil
        }
        guard popover != nil, popover.isShown else { return }
        popover.performClose(nil)
        popover.contentViewController = nil
    }

    private func setupKeyboardMonitors() {
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self = self else { return event }
            if event.keyCode == 49 && !event.isARepeat { // Space
                if !(self.window.firstResponder is NSTextView) {
                    self.store.beginHold()
                    return nil
                }
            }
            return event
        }
        NSEvent.addLocalMonitorForEvents(matching: .keyUp) { [weak self] event in
            guard let self = self else { return event }
            if event.keyCode == 49 {
                self.store.endHold()
            }
            return event
        }
    }
}

@main
struct PurrMacOSMain {
    static let delegate = PurrAppDelegate()
    static func main() {
        let app = NSApplication.shared
        app.delegate = delegate
        app.run()
    }
}
