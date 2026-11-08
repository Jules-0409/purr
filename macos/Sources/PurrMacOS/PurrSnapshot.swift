import SwiftUI
import AppKit

/// Hidden debug entry: `PURR_SNAPSHOT_DIR=/abs/dir Purr` renders every page off-screen
/// to PNG with demo data (same fixture as the Slint `--preview`) and exits.
/// It never starts the core, touches the system proxy or needs screen-recording permission.
/// Single time source for every time-driven view. Snapshot mode can pin it so a frame is
/// reproducible (ear position, breathing, zzz, icon hover waves); live builds use CACurrentMediaTime.
enum PurrClock {
    static var override: Double? = nil
    static func now() -> Double { override ?? CACurrentMediaTime() }
    static func ref(_ d: Date) -> Double { override ?? d.timeIntervalSinceReferenceDate }
}

/// Lets leaf views (bracket buttons, route cards) re-render when a snapshot script forces hover.
final class PurrHoverBus: ObservableObject {
    static let shared = PurrHoverBus()
    @Published var key: String? = nil
}

enum PurrSnapshot {
    /// Snapshot-only forced hover: a key such as "mode:rule", "tab:2", "bk:测速", "row:开机自启".
    static var hover: String? = nil { didSet { PurrHoverBus.shared.key = hover } }
    static func forced(_ key: String) -> Bool { active && hover == key }
    static let dir: String? = {
        let v = ProcessInfo.processInfo.environment["PURR_SNAPSHOT_DIR"]
        return (v?.isEmpty ?? true) ? nil : v
    }()
    static var active: Bool { dir != nil }

    static let demoNodes: [(String, String, Int, Int, Int, Double)] = [
        ("东京 01", "Hysteria2", 32, 3, 0, 99.9),
        ("东京 02", "Trojan", 38, 5, 0, 99.7),
        ("大阪 01", "VLESS", 41, 6, 1, 99.2),
        ("香港 03", "Hysteria2", 21, 2, 0, 99.9),
        ("香港 05", "Trojan", 27, 4, 0, 99.8),
        ("新加坡 02", "VLESS", 48, 9, 0, 99.5),
        ("首尔 01", "Trojan", 58, 7, 2, 98.6),
        ("台北 02", "VMess", 66, 24, 7, 94.1),
        ("洛杉矶 01", "Hysteria2", 156, 11, 0, 99.0),
        ("法兰克福 01", "Trojan", 188, 18, 3, 97.3),
        ("伦敦 02", "VLESS", 212, 14, 1, 98.8)
    ]

    static func seed(_ s: PurrStore) {
        var seedV: UInt64 = 7
        func rnd() -> Double { seedV = seedV * 16807 % 2147483647; return Double(seedV) / 2147483647.0 }
        s.nodes = demoNodes.enumerated().map { i, n in
            let hist: [Int?] = (0..<18).map { _ in
                if rnd() * 100 < Double(n.4) * 2.2 { return nil }
                return max(8, Int((Double(n.2) + (rnd() + rnd() - 1) * Double(n.3) * 1.6).rounded()))
            }
            return PurrNode(id: i, name: n.0, proto: n.1, ms: n.2, jitter: n.3, loss: n.4, uptime: n.5, history: hist)
        }
        s.groups = [
            PurrGroup(id: 0, name: "节点选择", selected: 0, nodeIds: Array(0...10)),
            PurrGroup(id: 1, name: "自动选择", selected: 3, nodeIds: [0, 1, 3, 4, 5]),
            PurrGroup(id: 2, name: "流媒体", selected: 7, nodeIds: [3, 4, 7, 8]),
            PurrGroup(id: 3, name: "AI 服务", selected: 8, nodeIds: [6, 8, 9, 10])
        ]
        s.subscriptions = [
            PurrSubItem(id: 1, name: "橘猫机场", desc: "11 个窝点 · 上次更新 刚刚", used: 62, total: 100, exp: "2027-03-12", active: true, daysLeft: 127),
            PurrSubItem(id: 2, name: "小黑猫 · 备用", desc: "6 个窝点 · 上次更新 昨天", used: 8, total: 50, exp: "2027-06-01", active: false)
        ]
        let rules: [(String, String, String)] = [
            ("DOMAIN-SUFFIX", "openai.com", "AI 服务"), ("DOMAIN-SUFFIX", "anthropic.com", "AI 服务"),
            ("DOMAIN-SUFFIX", "claude.ai", "AI 服务"), ("DOMAIN-SUFFIX", "youtube.com", "流媒体"),
            ("DOMAIN-SUFFIX", "netflix.com", "流媒体"), ("DOMAIN-SUFFIX", "github.com", "节点选择"),
            ("DOMAIN-KEYWORD", "google", "节点选择"), ("DOMAIN-SUFFIX", "notion.so", "节点选择"),
            ("GEOSITE", "category-ads-all", "REJECT"), ("GEOSITE", "cn", "DIRECT"),
            ("GEOIP", "LAN", "DIRECT"), ("GEOIP", "CN", "DIRECT"),
            ("PROCESS-NAME", "Steam", "DIRECT"), ("MATCH", "*", "节点选择")
        ]
        s.allRules = rules.enumerated().map { i, r in PurrRuleItem(id: i + 1, kind: r.0, match: r.1, policy: r.2, hits: 0) }
        s.logs = [
            PurrLog(time: "17:58:02", level: "purr", msg: "哄猫睡觉，断开连接"),
            PurrLog(time: "14:05:34", level: "info", msg: "自动选择 → 香港 03"),
            PurrLog(time: "14:05:33", level: "warn", msg: "东京 01 有点凉（286ms），换窝到 香港 03"),
            PurrLog(time: "11:40:19", level: "info", msg: "抓到一只老鼠 → doubleclick.net"),
            PurrLog(time: "08:12:04", level: "purr", msg: "摸醒了猫，连接到 东京 01")
        ]
        s.realExitIp = "203.0.113.11"
        s.realExitLoc = "东京 · JP"
        s.directIp = "198.51.100.7"
        s.directLoc = "上海 · CN · 本地宽带"
        s.todayTotal = "3.28 GB"
        s.todayDown = "2.62 GB"
        s.todayUp = "0.66 GB"
        s.routeProxy = "2.09 GB"
        s.routeDirect = "1.19 GB"
        s.routeProxyFraction = 2090.0 / 3280.0
        s.blockedMice = 1284
        s.memMB = 22
        s.statsReady = true
    }

    static func setState(_ s: PurrStore, _ state: String, page: Int, pane: String = "live") {
        let awake = !["sleep", "first", "dark", "traysleep"].contains(state)
        s.firstRun = state == "first"
        if state == "first" { s.subscriptions = [] } else if s.subscriptions.isEmpty { seed(s) }
        s.errKind = ["timeout", "offline", "sub", "port"].contains(state) ? state : nil
        s.mode = ["global", "direct"].contains(state) ? state : "rule"
        s.isAwake = awake
        s.floodAnimating = false
        s.connectedAt = Date().addingTimeInterval(-3725)
        s.elapsedStr = awake ? "01:02:05" : "00:00:00"
        s.memMB = awake ? 54.2 : 22
        let ok = awake && s.errKind == nil
        s.dnSpeed = ok ? 11.7 : 0
        s.upSpeed = ok ? 2.9 : 0
        s.load = ok ? min(1, 11.7 / 16) : 0
        var hd: [Double] = []
        for i in 0..<PurrStore.historySamples {
            let x = Double(i)
            let v: Double = 8.0 + 5.0 * sin(x / 11.0) + 2.0 * sin(x / 3.0)
            hd.append(ok ? v : 0.0)
        }
        s.histDn = hd
        s.histUp = s.histDn.map { $0 * 0.26 }
        s.histCount = ok ? PurrStore.historySamples : 0
        s.peakDn = s.histDn.max() ?? 0
        s.peakUp = s.histUp.max() ?? 0
        let hosts: [(String, String, String)] = [
            ("www.youtube.com", "流媒体", "东京 01"), ("api.openai.com", "AI 服务", "洛杉矶 01"),
            ("github.com", "节点选择", "东京 01"), ("www.notion.so", "节点选择", "东京 01"),
            ("claude.ai", "AI 服务", "洛杉矶 01"), ("www.bilibili.com", "DIRECT", "直连"),
            ("discord.com", "节点选择", "东京 01")
        ]
        var cs: [PurrConn] = []
        if ok {
            for (i, h) in hosts.enumerated() {
                let direct: Bool = s.mode == "direct" || (s.mode == "rule" && h.1 == "DIRECT")
                let app: String = i == 1 ? "ChatGPT" : (i == 2 ? "Cursor" : "Chrome")
                let rule: String = s.mode == "rule" ? "DOMAIN-SUFFIX" : "MODE · \(s.mode)"
                let dn = Double(420 + i * 91)
                let up = Double(24 + i * 12)
                let age = Double(12 + i * 8)
                cs.append(PurrConn(id: i + 1, rawId: "\(i)", app: app, host: h.0, rule: rule,
                                   pol: direct ? "DIRECT" : h.1, chain: direct ? "直连" : h.2,
                                   dn: dn, up: up, age: age))
            }
        }
        s.conns = cs
        s.connPane = pane
        s.page = page
        s.prevPage = page
        s.pageChangedAt = 0
    }

    // name, state, page, pane, dark
    static let frames: [(String, String, Int, String, Bool)] = [
        ("sleep", "sleep", 0, "live", false),
        ("awake", "ok", 0, "live", false),
        ("nodes", "ok", 1, "live", false),
        ("subscriptions", "ok", 2, "live", false),
        ("connections", "ok", 3, "live", false),
        ("logs", "ok", 3, "log", false),
        ("rules", "ok", 4, "live", false),
        ("settings", "ok", 5, "live", false),
        ("sleep-nodes", "sleep", 1, "live", false),
        ("first", "first", 0, "live", false),
        ("empty-subscriptions", "first", 2, "live", false),
        ("timeout", "timeout", 0, "live", false),
        ("sub-expired", "sub", 0, "live", false),
        ("global", "global", 0, "live", false),
        ("direct", "direct", 0, "live", false),
        ("dark", "dark", 0, "live", true)
    ]

    @MainActor
    static func render<V: View>(_ view: V, size: CGSize, to path: String, dark: Bool) {
        let host = NSHostingView(rootView: view.environment(\.colorScheme, dark ? .dark : .light))
        host.frame = CGRect(origin: .zero, size: size)
        let win = NSWindow(contentRect: CGRect(x: -20000, y: -20000, width: size.width, height: size.height),
                           styleMask: [.borderless], backing: .buffered, defer: false)
        win.contentView = host
        win.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        host.layoutSubtreeIfNeeded()
        host.display()
        // Prefer the real window layer tree (renders AppKit-backed controls too).
        if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
            host.cacheDisplay(in: host.bounds, to: rep)
            if let data = rep.representation(using: .png, properties: [:]) {
                try? data.write(to: URL(fileURLWithPath: path.replacingOccurrences(of: ".png", with: ".cd.png")))
            }
        }
        let r = ImageRenderer(content: view.environment(\.colorScheme, dark ? .dark : .light).frame(width: size.width, height: size.height))
        r.scale = 2
        if let cg = r.cgImage {
            let rep = NSBitmapImageRep(cgImage: cg)
            if let data = rep.representation(using: .png, properties: [:]) {
                try? data.write(to: URL(fileURLWithPath: path))
            }
        }
        win.contentView = nil
        print("snapshot: \(path)")
    }

    /// `PURR_SNAPSHOT_SCRIPT=steps.txt`: parity script (same verbs as the Slint `--script`), run in an
    /// on-screen window so SwiftUI animations really tick; frames are cropped at 2x and named
    /// `<name>-<ms since last action>` for bursts.
    @MainActor
    static func runScript(store: PurrStore, path: String, dir: String) {
        let text = (try? String(contentsOfFile: path, encoding: .utf8)) ?? ""
        let steps = text.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .map { $0.split(separator: " ").map(String.init) }
        let host = NSHostingView(rootView: AnyView(PurrMainView(store: store).environment(\.colorScheme, .light)))
        host.frame = CGRect(x: 0, y: 0, width: 1200, height: 760)
        let win = NSWindow(contentRect: CGRect(x: 40, y: 40, width: 1200, height: 760),
                           styleMask: [.borderless], backing: .buffered, defer: false)
        win.contentView = host
        win.appearance = NSAppearance(named: .aqua)
        win.level = .floating
        win.orderFrontRegardless()
        var frames: [(String, NSBitmapImageRep)] = []
        var mark = CACurrentMediaTime()
        func grab(_ a: [String]) -> NSBitmapImageRep? {
            let n = { (i: Int) -> CGFloat in CGFloat(Double(a.count > i ? a[i] : "0") ?? 0) }
            let r = CGRect(x: n(2), y: n(3), width: n(4), height: n(5))
            guard let rep = host.bitmapImageRepForCachingDisplay(in: r) else { return nil }
            host.cacheDisplay(in: r, to: rep)
            return rep
        }
        func finish() {
            for (name, rep) in frames {
                if let d = rep.representation(using: .png, properties: [:]) {
                    try? d.write(to: URL(fileURLWithPath: "\(dir)/\(name).png"))
                }
            }
            print("script: \(frames.count) frames"); exit(0)
        }
        func step(_ i: Int) {
            if i >= steps.count { finish(); return }
            let a = steps[i]
            let arg = { (k: Int) -> String in a.count > k ? a[k] : "" }
            var next: Double = 0
            switch a[0] {
            case "state": setState(store, arg(1), page: store.page); store.page = Int(arg(2)) ?? store.page
            case "page": store.page = Int(arg(1)) ?? 0
            case "pane": store.connPane = arg(1) == "1" ? "log" : "live"
            case "hover": hover = arg(1) == "none" ? nil : arg(1); store.objectWillChange.send()
            case "node": store.hoveredNodeId = arg(1) == "none" ? nil : Int(arg(1))
            case "clock": PurrClock.override = arg(1) == "live" ? nil : Double(arg(1)); store.objectWillChange.send()
            case "toast": store.showToast(a.dropFirst().joined(separator: " "))
            case "wake": store.wake()
            case "sleep": store.sleep()
            case "hold": store.beginHold()
            case "release": store.endHold()
            case "setting":
                switch arg(1) {
                case "autoSwap": store.autoSwap.toggle()
                case "blockAds": store.blockAds.toggle()
                default: store.autoWake.toggle()
                }
            case "mark": break
            case "wait": next = (Double(arg(1)) ?? 0) / 1000
            case "cap": if let r = grab(a) { frames.append((arg(1), r)) }
            case "burst":
                let until = CACurrentMediaTime() + (Double(arg(6)) ?? 0) / 1000
                func tick() {
                    let now = CACurrentMediaTime()
                    if now >= until { step(i + 1); return }
                    let t = Int((now - mark) * 1000)
                    if let r = grab(a) { frames.append((String(format: "%@-%05d", arg(1), t), r)) }
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.004) { tick() }
                }
                tick(); return
            default: print("script: unknown step \(a[0])")
            }
            if !["wait", "cap"].contains(a[0]) { mark = CACurrentMediaTime() }
            if next > 0 {
                DispatchQueue.main.asyncAfter(deadline: .now() + next) { step(i + 1) }
            } else {
                DispatchQueue.main.async { step(i + 1) }
            }
        }
        seed(store)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { step(0) }
    }

    /// `PURR_SNAPSHOT_REAL=unavailable` or `=/abs/snapshot.json`: renders every page through the
    /// real `applySnapshot` path instead of the demo fixture — first with nothing measured, then
    /// (for a file) with a snapshot the running core produced (`debug-snapshot.json`).
    @MainActor
    static func runReal(store: PurrStore, source: String, dir: String) {
        let pages: [(String, Int, String)] = [
            ("overview", 0, "live"), ("nodes", 1, "live"), ("subscriptions", 2, "live"),
            ("connections", 3, "live"), ("logs", 3, "log"), ("rules", 4, "live"), ("settings", 5, "live")
        ]
        var queue: [(String, () -> Void, Bool)] = []
        for p in pages {
            queue.append(("mac-real-unavailable-\(p.0)", { store.isAwake = true; store.page = p.1; store.connPane = p.2 }, false))
        }
        queue.append(("mac-real-unavailable-tray", { store.page = 0 }, true))
        if source != "unavailable", let json = try? String(contentsOfFile: source, encoding: .utf8) {
            for p in pages {
                queue.append(("mac-real-populated-\(p.0)", {
                    store.isAwake = true
                    store.connectedAt = Date().addingTimeInterval(-754)
                    store.applySnapshot(json, sampleTraffic: p.0 == "overview")
                    store.page = p.1; store.connPane = p.2
                }, false))
            }
            queue.append(("mac-real-populated-tray", { store.page = 0 }, true))
            queue.append(("mac-real-populated-detail", { store.page = 0; store.showServiceDetail = true }, false))
        }
        func step(_ i: Int) {
            if i >= queue.count { print("snapshot: done"); exit(0) }
            let (name, apply, tray) = queue[i]
            apply()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.9) {
                let path = "\(dir)/\(name).png"
                if tray {
                    render(PurrTrayPopoverView(store: store, onOpenMain: {}).fixedSize(), size: CGSize(width: 308, height: 470), to: path, dark: false)
                } else {
                    render(PurrMainView(store: store), size: CGSize(width: 1200, height: 760), to: path, dark: false)
                }
                step(i + 1)
            }
        }
        step(0)
    }

    @MainActor
    static func run(store: PurrStore) {
        guard let dir = dir else { return }
        if let real = ProcessInfo.processInfo.environment["PURR_SNAPSHOT_REAL"], !real.isEmpty {
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
            runReal(store: store, source: real, dir: dir)
            return
        }
        if let script = ProcessInfo.processInfo.environment["PURR_SNAPSHOT_SCRIPT"], !script.isEmpty {
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
            runScript(store: store, path: script, dir: dir)
            return
        }
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        seed(store)
        var queue = frames.map { f in (f.0, { setState(store, f.1, page: f.2, pane: f.3) }, f.4, false) }
        queue.append(("tray-awake", { setState(store, "ok", page: 0) }, false, true))
        queue.append(("tray-sleep", { setState(store, "sleep", page: 0) }, false, true))
        queue.append(("tray-first", { setState(store, "first", page: 0) }, false, true))
        if let only = ProcessInfo.processInfo.environment["PURR_SNAPSHOT_ONLY"], !only.isEmpty {
            queue = queue.filter { $0.0.hasPrefix(only) }
        }
        let only = ProcessInfo.processInfo.environment["PURR_SNAPSHOT_ONLY"] ?? ""
        if only.isEmpty || "disclaimer".hasPrefix(only) || only.hasPrefix("disclaimer") {
            for (name, text) in [("disclaimer-empty", ""), ("disclaimer-typed", PurrDisclaimer.phrase)] {
                render(PurrDisclaimerView(input: .constant(text), onAccept: {}, onQuit: {}),
                       size: PurrDisclaimer.size, to: "\(dir)/mac-\(name).png", dark: false)
            }
        }
        func step(_ i: Int) {
            if i >= queue.count { print("snapshot: done"); exit(0) }
            let (name, apply, dark, tray) = queue[i]
            apply()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.9) {
                let path = "\(dir)/mac-\(name).png"
                if tray {
                    render(PurrTrayPopoverView(store: store, onOpenMain: {}).fixedSize(), size: CGSize(width: 308, height: 470), to: path, dark: dark)
                } else {
                    render(PurrMainView(store: store), size: CGSize(width: 1200, height: 760), to: path, dark: dark)
                }
                step(i + 1)
            }
        }
        step(0)
    }
}
