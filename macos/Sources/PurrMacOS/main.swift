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
        let validIds = (store.groups.first?.nodeIds ?? []).filter { store.nodes.indices.contains($0) }
        let sortedTop5 = Array(validIds.sorted { store.nodes[$0].sortMs < store.nodes[$1].sortMs }.prefix(5))

        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(store.firstRun ? "没有窝" : (store.errKind != nil ? "卡住了" : (store.isAwake ? (store.mode == "direct" ? "醒着 · 直连" : "呼噜中") : "睡着了")))
                        .font(.system(size: 15, weight: .heavy))
                        .foregroundColor(theme.ink)
                    Text(store.firstRun ? "先导入订阅" : (store.errKind != nil ? store.troubleInfo(for: store.errKind!).st : (store.isAwake ? (store.mode == "direct" ? "不经过窝点 · \(store.exitInfo().ip)" : (store.groups.isEmpty ? "—" : "\(n.name) · \(n.msText)") + " · \(store.exitInfo().ip)") : "系统代理已关 · 端口 \(store.mixedPort) 仍在")))
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
                // The real name of the group these members belong to (hidden until the core sent one).
                if let groupName = store.groups.first?.name {
                    Text(groupName).font(.system(size: 10, design: .monospaced)).tracking(1.5).foregroundColor(theme.mute)
                }
                ForEach(sortedTop5, id: \.self) { i in
                    let nd = store.nodes[i]
                    let sel = store.groups.first?.selected == i
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
                    Text("↑ " + store.rateText(store.upSpeed))
                    Text("↓ " + store.rateText(store.dnSpeed))
                        .fontWeight(.bold)
                        .foregroundColor(theme.ink)
                }
                Spacer()
                Text(store.memMB.map { String(format: "内存 %.0f MB", $0) } ?? "内存 —")
                Spacer()
                Text("今日 \(store.todayTotal ?? "—")")
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

struct PurrDisclaimerHost: View {
    let store: PurrStore
    let onAccepted: () -> Void
    let onQuit: () -> Void
    @State private var input = ""
    @State private var failed = false

    var body: some View {
        PurrDisclaimerView(input: $input, failed: failed, onAccept: {
            guard PurrDisclaimer.matches(input) else { return }
            if store.acceptDisclaimer(input) { onAccepted() } else { failed = true }
        }, onQuit: onQuit)
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
        installTerminationSignals()
        if let icnsPath = Bundle.main.path(forResource: "AppIcon", ofType: "icns"),
           let iconImg = NSImage(contentsOfFile: icnsPath) {
            NSApp.applicationIconImage = iconImg
        } else if let fallbackImg = NSImage(contentsOfFile: "/Applications/Purr.app/Contents/Resources/AppIcon.icns") {
            NSApp.applicationIconImage = fallbackImg
        }

        if store.disclaimerPending {
            showDisclaimer()
            return
        }
        launchMainUI()
    }

    private var disclaimerWindow: NSWindow?
    private var signalSources: [DispatchSourceSignal] = []

    /// `kill` / `killall Purr` (and app updaters) send SIGTERM; route it through NSApp.terminate so
    /// applicationWillTerminate stops the core instead of leaving it orphaned with the ports.
    private func installTerminationSignals() {
        for sig in [SIGTERM, SIGINT, SIGHUP] {
            signal(sig, SIG_IGN)
            let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            src.setEventHandler { NSApp.terminate(nil) }
            src.resume()
            signalSources.append(src)
        }
    }

    private func showDisclaimer() {
        let size = PurrDisclaimer.size
        let win = PurrKeyWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        win.isOpaque = false
        win.backgroundColor = .clear
        win.hasShadow = true
        win.isMovableByWindowBackground = true
        win.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: PurrDisclaimerHost(store: store,
            onAccepted: { [weak self] in self?.disclaimerAccepted() },
            onQuit: { NSApp.terminate(nil) }))
        host.wantsLayer = true
        host.layer?.cornerRadius = 12
        host.layer?.cornerCurve = .continuous
        host.layer?.masksToBounds = true
        win.contentView = host
        win.center()
        disclaimerWindow = win
        win.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func disclaimerAccepted() {
        disclaimerWindow?.orderOut(nil)
        disclaimerWindow?.contentView = nil
        disclaimerWindow = nil
        launchMainUI()
        NSApp.activate(ignoringOtherApps: true)
    }

    private func launchMainUI() {
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 760),
            styleMask: [.borderless, .fullSizeContentView, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
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
            self?.refreshWindowVisibility()
        }
        // Display asleep / screen locked / fast user switch: nothing is drawn, so stop animating.
        let ws = NSWorkspace.shared.notificationCenter
        for name in [NSWorkspace.screensDidSleepNotification, NSWorkspace.sessionDidResignActiveNotification] {
            ws.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.screensAwake = false
                self?.refreshWindowVisibility()
            }
        }
        for name in [NSWorkspace.screensDidWakeNotification, NSWorkspace.sessionDidBecomeActiveNotification] {
            ws.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.screensAwake = true
                self?.refreshWindowVisibility()
            }
        }
        refreshWindowVisibility()
    }

    private var screensAwake = true

    private func refreshWindowVisibility() {
        guard let window = window else { return }
        let visible = screensAwake && window.isVisible && window.occlusionState.contains(.visible)
            && window.contentView is NSHostingView<PurrMainView>
        if PurrVisibility.shared.mainWindowVisible != visible {
            PurrVisibility.shared.mainWindowVisible = visible
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        if PurrSnapshot.active || store.disclaimerPending { return }
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
        // With isMovableByWindowBackground on, AppKit re-walks the whole SwiftUI responder tree to
        // rebuild the drag regions on every display cycle (~20 % of the main thread while animating).
        // Only keep it on while the pointer is over the window, which is the only time a drag can start.
        window.isMovableByWindowBackground = false
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: dragToggle,
            userInfo: nil
        )
        dragToggle.window = window
        hostingView.addTrackingArea(area)
        if let w = window, w.frame.contains(NSEvent.mouseLocation) {
            w.isMovableByWindowBackground = true
        }
    }

    private let dragToggle = PurrDragToggle()

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
        DispatchQueue.main.async { [weak self] in self?.refreshWindowVisibility() }
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

    /// What the menu-bar image last showed; the image is only rebuilt when this changes.
    /// Light/dark menu bars and Retina vs standard screens need no key: the image is a template
    /// (AppKit tints it) and carries both 1x and 2x bitmaps.
    private var statusImageKey: String = ""

    private func updateStatusItemAppearance() {
        guard Thread.isMainThread else {
            DispatchQueue.main.async { [weak self] in self?.updateStatusItemAppearance() }
            return
        }
        guard let btn = statusItem?.button else { return }
        let awake = store.isAwake && !store.firstRun && !store.disclaimerPending
        let hasError = store.errKind != nil
        let showSpeed = awake && !hasError && store.statsReady
        let speed: (up: String, dn: String)? = showSpeed
            ? ("↑" + PurrRate.compact(store.upSpeed), "↓" + PurrRate.compact(store.dnSpeed))
            : nil
        let key = "\(awake)|\(hasError)|" + (speed.map { $0.up + "|" + $0.dn } ?? "-")
        if key == statusImageKey, btn.image != nil { return }
        statusImageKey = key
        if !btn.title.isEmpty { btn.title = "" }
        if btn.imagePosition != .imageOnly { btn.imagePosition = .imageOnly }
        // Drawn now, into bitmaps; AppKit's status-bar redraws only blit pixels (see PurrStatusImage).
        btn.image = PurrStatusImage.make(isAwake: awake, hasError: hasError, speed: speed)
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

/// Turns window-background dragging on only while the pointer is inside the main window.
final class PurrDragToggle: NSResponder {
    weak var window: NSWindow?

    override func mouseEntered(with event: NSEvent) {
        window?.isMovableByWindowBackground = true
    }

    override func mouseExited(with event: NSEvent) {
        // Mid-drag the pointer can lag outside the frame for a moment; keep it on while the button is down.
        if NSEvent.pressedMouseButtons & 1 != 0 { return }
        window?.isMovableByWindowBackground = false
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
