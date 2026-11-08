import SwiftUI
import Combine
import AppKit
import AVFoundation
import Darwin
import PurrCore

// MARK: - Theme Tokens (matching PURR-desktop-v5.html :root)
/// Speed text. Below 1 MB/s it switches to KB/s: one decimal of MB/s turns every ordinary
/// browsing rate (under ~50 KB/s) into "0.0".
/// Decimal units everywhere (1 MB = 10^6 B, same rule as the Rust `present` module).
enum PurrRate {
    static func parts(_ mbPerSec: Double) -> (String, String) {
        if mbPerSec >= 1.0 { return (String(format: "%.1f", mbPerSec), "MB/s") }
        return (String(format: "%.0f", max(0, mbPerSec) * 1000.0), "KB/s")
    }
    /// Menu-bar form, at most 5 characters after the arrow.
    static func compact(_ mbPerSec: Double) -> String {
        if mbPerSec >= 1.0 { return String(format: "%4.1fM", mbPerSec) }
        return String(format: "%4.0fK", max(0, mbPerSec) * 1000.0)
    }
    /// Whole decimal GB as providers print quotas; one decimal below 10 GB (`present::gb_text`).
    static func gb(_ v: Double) -> String {
        v < 10 ? String(format: "%.1f", v) : String(format: "%.0f", v)
    }
}

struct PurrTheme {
    let bg: Color
    let ink: Color
    let mute: Color
    let line: Color
    let warm: Color

    static func current(isAwake: Bool, mode: String, hasErr: Bool, isDark: Bool) -> PurrTheme {
        let warmColor: Color = {
            if hasErr {
                return Color(red: 0xE9/255.0, green: 0xA8/255.0, blue: 0x8A/255.0) // #E9A88A
            }
            switch mode {
            case "global":
                return Color(red: 1.0, green: 0x6B/255.0, blue: 0x3D/255.0) // #FF6B3D
            case "direct":
                return Color(red: 1.0, green: 0xD2/255.0, blue: 0xAE/255.0) // #FFD2AE
            default:
                return Color(red: 1.0, green: 0x8A/255.0, blue: 0x4C/255.0) // #FF8A4C
            }
        }()

        let sleepBg = isDark
            ? Color(red: 0x15/255.0, green: 0x13/255.0, blue: 0x1B/255.0)
            : Color(red: 0xE7/255.0, green: 0xE5/255.0, blue: 0xED/255.0)
        let sleepInk = isDark
            ? Color(red: 0xEE/255.0, green: 0xEC/255.0, blue: 0xF4/255.0)
            : Color(red: 0x1C/255.0, green: 0x19/255.0, blue: 0x26/255.0)
        let sleepMute = isDark
            ? Color(red: 0xEE/255.0, green: 0xEC/255.0, blue: 0xF4/255.0).opacity(0.58)
            : Color(red: 0x1C/255.0, green: 0x19/255.0, blue: 0x26/255.0).opacity(0.56)
        let sleepLine = isDark
            ? Color(red: 0xEE/255.0, green: 0xEC/255.0, blue: 0xF4/255.0).opacity(0.16)
            : Color(red: 0x1C/255.0, green: 0x19/255.0, blue: 0x26/255.0).opacity(0.16)

        if isAwake {
            let wInk = Color(red: 0x1F/255.0, green: 0x12/255.0, blue: 0x0A/255.0) // #1F120A
            let wMute = wInk.opacity(0.74)
            let wLine = wInk.opacity(0.20)
            return PurrTheme(bg: warmColor, ink: wInk, mute: wMute, line: wLine, warm: warmColor)
        } else {
            return PurrTheme(bg: sleepBg, ink: sleepInk, mute: sleepMute, line: sleepLine, warm: warmColor)
        }
    }
}

// MARK: - Models
struct PurrNode: Identifiable, Equatable {
    let id: Int
    var name: String
    var proto: String
    var ms: Int
    var jitter: Int
    var loss: Int
    var uptime: Double
    var history: [Int?]
    /// ms <= 0 means not measured yet: shown as "—" and sorted last (never a made-up number).
    var known: Bool { ms > 0 }
    var msText: String { ms > 0 ? "\(ms)ms" : "—" }
    var msValue: String { ms > 0 ? "\(ms)" : "—" }
    var sortMs: Int { ms > 0 ? ms : Int.max }
    /// Real probe samples behind jitter / loss / success rate (fewer than 2 = not enough to say).
    var samples: Int { history.compactMap { $0 }.count }
    var jitterText: String { samples >= 2 && jitter >= 0 ? "±\(jitter)" : "—" }
    var lossText: String { samples >= 2 && loss >= 0 ? "\(loss)" : "—" }
    var uptimeText: String { uptime >= 0 ? String(format: "%.1f", uptime) : "—" }

    var verdict: (title: String, desc: String, level: Int) {
        if samples < 2 {
            return ("样本不足", "多次测速后查看抖动与丢包", 0)
        }
        if loss >= 3 || jitter >= 20 {
            return ("常掉线", "不建议常驻，偶尔用用可以", 2)
        } else if jitter >= 10 || loss >= 1 {
            return ("有点抖", "看视频、语音偶尔会卡一下", 1)
        } else {
            return ("很稳", "适合长连接、游戏和通话", 0)
        }
    }
}

struct PurrGroup: Identifiable, Equatable {
    let id: Int
    var name: String
    var selected: Int
    var nodeIds: [Int]
}

struct PurrConn: Identifiable, Equatable {
    let id: Int
    var rawId: String = ""
    var app: String
    var host: String
    var rule: String
    var pol: String
    var chain: String
    var dn: Double
    var up: Double
    /// Seconds since the connection started; nil when the core's start time did not parse.
    var age: Double?
    var ageText: String = ""
    var ageLabel: String { !ageText.isEmpty ? ageText : (age.map { "\(Int($0))s" } ?? "—") }
}

struct PurrLog: Identifiable {
    let id = UUID()
    var time: String
    var level: String // "purr", "info", "warn"
    var msg: String
}

struct PurrRuleItem: Identifiable, Equatable {
    let id: Int
    var kind: String
    var match: String
    var policy: String
    /// The core's match count since it started (`extra.hitCount`); nil = not reported.
    var hits: Int?
}

struct PurrSubItem: Identifiable, Equatable {
    let id: Int
    var name: String
    var desc: String
    /// Decimal GB from the subscription's userinfo header (as the provider prints it); -1 = not said.
    var used: Double
    var total: Double
    var exp: String
    var active: Bool
    /// Days until the provider's expiry; nil = not provided.
    var daysLeft: Int? = nil
    var usedText: String { used >= 0 ? PurrRate.gb(used) : "—" }
    var totalText: String { total > 0 ? PurrRate.gb(total) : "—" }
    var fraction: Double { used >= 0 && total > 0 ? min(1.0, used / total) : 0 }
    var leftText: String { used >= 0 && total > 0 ? "剩 \(PurrRate.gb(max(0, total - used))) GB" : "额度未提供" }
    var expiryText: String {
        guard let d = daysLeft else { return "到期时间未提供" }
        return d <= 0 ? "已到期" : "\(d) 天后到期"
    }
}

// MARK: - Purr Audio Synthesizer
final class PurrSynth {
    private var engine: AVAudioEngine?
    private var mixer: AVAudioMixerNode?
    private var phase1: Double = 0
    private var phase2: Double = 0
    private var noiseState: Double = 0
    var targetGain: Float = 0.0

    func update(gain: Float) {
        targetGain = gain
        if gain > 0.001 {
            if engine == nil {
                startEngine()
            }
            mixer?.outputVolume = gain
        } else if let eng = engine {
            mixer?.outputVolume = 0
            eng.stop()
            engine = nil
            mixer = nil
            malloc_zone_pressure_relief(nil, 0)
        }
    }

    private func startEngine() {
        let eng = AVAudioEngine()
        let format = eng.outputNode.inputFormat(forBus: 0)
        let sr = format.sampleRate > 0 ? format.sampleRate : 44100.0
        let src = AVAudioSourceNode { [weak self] _, _, frameCount, audioBufferList -> OSStatus in
            guard let self = self else { return noErr }
            let abl = UnsafeMutableAudioBufferListPointer(audioBufferList)
            for frame in 0..<Int(frameCount) {
                let rnd = Double.random(in: -1.0...1.0)
                self.noiseState = (self.noiseState + 0.02 * rnd) / 1.02
                self.phase1 += 2.0 * .pi * 26.0 / sr
                self.phase2 += 2.0 * .pi * 0.45 / sr
                let am = 0.5 + 0.5 * sin(self.phase1)
                let br = 0.6 + 0.4 * sin(self.phase2)
                let sample = Float(self.noiseState * 3.5 * am * br * 0.25)
                for buf in abl {
                    let ptr = buf.mData?.assumingMemoryBound(to: Float.self)
                    ptr?[frame] = sample
                }
            }
            return noErr
        }
        let mix = AVAudioMixerNode()
        mix.outputVolume = 0
        eng.attach(src)
        eng.attach(mix)
        eng.connect(src, to: mix, format: format)
        eng.connect(mix, to: eng.mainMixerNode, format: format)
        try? eng.start()
        self.engine = eng
        self.mixer = mix
    }
}

// MARK: - PurrStore (Complete State Engine backed by Rust + mihomo)
final class PurrStore: ObservableObject {
    @Published var isAwake: Bool = false {
        didSet {
            if !PurrSnapshot.active { purr_core_set_awake(isAwake) }
            if isAwake != oldValue { updateServiceAutoProbe() }
        }
    }
    @Published var firstRun: Bool = false
    @Published var errKind: String? = nil // "timeout", "offline", "sub", "port"
    @Published var badNodeId: Int = 0
    @Published var demoState: String = "ok"

    @Published var mode: String = "rule" {
        didSet {
            if !PurrSnapshot.active { mode.withCString { purr_core_set_mode($0) } }
        }
    }
    @Published var sysProxy: Bool = true {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("system-proxy", sysProxy ? "true" : "false")
        }
    }
    @Published var tunEnabled: Bool = false
    private var isHydratingPrefs: Bool = false
    @Published var page: Int = 0 {
        didSet {
            prevPage = oldValue
            pageChangedAt = PurrClock.now()
            // 打开窝点页时自动测一次速（未测过的显示「—」，不再用假数字）
            if page == 1 && oldValue != 1 { autoSpeedTest() }
            // 概览页常用网站：进概览自动测一次，停在概览时每 60 秒重测（离开即停）
            if page != oldValue { updateServiceAutoProbe() }
        }
    }
    var prevPage: Int = 0
    var pageChangedAt: TimeInterval = 0

    // Ear & Flood physics
    @Published var floodX: CGFloat = 100.0
    @Published var floodY: CGFloat = 110.0
    @Published var floodAnimating: Bool = false
    @Published var floodRadius: CGFloat = 0.0
    private var floodStart: TimeInterval = 0
    private var floodTargetAwake: Bool = false
    @Published var pinnedOnTop: Bool = false {
        didSet { onTogglePin?(pinnedOnTop) }
    }
    var onTogglePin: ((Bool) -> Void)? = nil
    var onSuspendWindow: (() -> Void)? = nil
    var onTrayUpdate: (() -> Void)? = nil

    func currentEarX(width: Double, now: TimeInterval) -> (ex: Double, speed: Double) {
        let targetX = width * (Double(page) + 0.5) / Double(pageTabs.count)
        let startX = width * (Double(prevPage) + 0.5) / Double(pageTabs.count)
        let dt = max(0.0, now - pageChangedAt)
        if dt >= 0.55 || prevPage == page {
            return (targetX, 0.0)
        }
        let progress = 1.0 - exp(-9.5 * dt) * cos(14.0 * dt)
        let x = startX + (targetX - startX) * progress
        let vel = abs(targetX - startX) * exp(-9.5 * dt)
        return (x, min(1.0, vel / 260.0))
    }

    // Hold to connect
    @Published var holding: Bool = false
    @Published var holdProgress: Double = 0.0
    private var holdStart: TimeInterval = 0
    @Published var nudgeTip: Bool = false

    // Live metrics (synced from mihomo via purr_core_snapshot_json)
    @Published var dnSpeed: Double = 0.0
    /// The core's controller answered the last poll; speeds show "—" otherwise.
    @Published var statsReady: Bool = false
    /// First-launch notice not accepted yet: the core, system proxy and TUN stay untouched.
    @Published var disclaimerPending: Bool = false
    @Published var upSpeed: Double = 0.0
    @Published var load: Double = 0.0
    @Published var peakDn: Double = 0.0
    @Published var peakUp: Double = 0.0
    /// Today's traffic (local calendar day, persisted by the core in statistics.json), already
    /// formatted with its unit ("1.50 GB"); nil until the core answered once this run.
    @Published var todayTotal: String? = nil
    @Published var todayDown: String? = nil
    @Published var todayUp: String? = nil
    @Published var routeProxy: String? = nil
    @Published var routeDirect: String? = nil
    /// Proxy share of today's routed bytes; nil when nothing was routed yet.
    @Published var routeProxyFraction: Double? = nil
    /// Rule-rejected connections today; nil until the core's log stream connected.
    @Published var blockedMice: Int? = nil
    /// Decimal MB of Purr + core; nil until both were measured.
    @Published var memMB: Double? = nil
    @Published var memShellMB: Double? = nil
    @Published var memCoreMB: Double? = nil
    /// CPU % of Purr + core (100 % = one core, like Activity Monitor); nil until two samples.
    @Published var cpuPct: Double? = nil
    @Published var connectedAt: Date = Date()
    /// "—" until a session has been timed (no nap yet this run).
    @Published var elapsedStr: String = "—"
    /// One sample per second while awake: exactly the last 60 s; cleared on sleep.
    static let historySamples = 60
    @Published var histDn: [Double] = Array(repeating: 0.0, count: PurrStore.historySamples)
    @Published var histUp: [Double] = Array(repeating: 0.0, count: PurrStore.historySamples)
    /// Samples recorded since the cat woke up (peak is "—" until there is one).
    @Published var histCount: Int = 0

    // Real Exit IP & Direct IP
    @Published var realExitIp: String = "—"
    @Published var realExitLoc: String = ""
    /// "pending" (looking up), "ok", "failed" — from the core.
    @Published var exitState: String = "pending"
    @Published var directIp: String = "—"
    @Published var directLoc: String = ""
    @Published var directState: String = "pending"
    /// The version the Rust core was built as (Cargo `CARGO_PKG_VERSION`).
    static let appVersion: String = {
        guard let p = purr_core_version() else { return "—" }
        defer { purr_string_free(p) }
        return String(cString: p)
    }()
    private var nodeHistories: [String: [Int?]] = [:]

    // Nodes & Groups
    @Published var nodes: [PurrNode] = []
    /// Empty until the core's first real snapshot: no placeholder group or node names.
    @Published var groups: [PurrGroup] = []
    @Published var selectedGroupIdx: Int = 0 {
        didSet { if selectedGroupIdx != oldValue && page == 1 { autoSpeedTest() } }
    }
    private var lastAutoTest: [String: TimeInterval] = [:]
    /// Auto test on entering the nodes page / picking a group; at most once per group every 20 s.
    func autoSpeedTest() {
        guard !PurrSnapshot.active, isAwake, !isTestingNodes, groups.indices.contains(selectedGroupIdx) else { return }
        let name = groups[selectedGroupIdx].name
        let now = CACurrentMediaTime()
        if let t = lastAutoTest[name], now - t < 20 { return }
        lastAutoTest[name] = now
        triggerSpeedTest(quiet: true)
    }
    @Published var hoveredNodeId: Int? = nil
    @Published var isTestingNodes: Bool = false

    // Subscriptions
    @Published var subInputUrl: String = ""
    @Published var firstRunUrl: String = ""
    @Published var shakeCount: Int = 0
    @Published var isImporting: Bool = false
    @Published var subscriptions: [PurrSubItem] = []

    // Connections & Logs
    @Published var connPane: String = "live" // "live" or "log"
    @Published var connQuery: String = ""
    @Published var conns: [PurrConn] = []
    @Published var logLevelFilter: String = "all" // "all", "purr", "warn"
    @Published var logs: [PurrLog] = []

    // Rules
    @Published var ruleQuery: String = ""
    @Published var allRules: [PurrRuleItem] = []

    // Settings (backed by Rust Preferences in ~/Library/Application Support/Purr/preferences.json & config.yaml)
    @Published var autoWake: Bool = false {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("auto-wake", autoWake ? "true" : "false")
        }
    }
    @Published var autoSwap: Bool = true {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("auto-swap", autoSwap ? "true" : "false")
        }
    }
    @Published var blockAds: Bool = true {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("block-ads", blockAds ? "true" : "false")
        }
    }
    @Published var closeAction: String = "tray" { // "tray", "quit"
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("close-to-tray", closeAction == "tray" ? "true" : "false")
        }
    }
    @Published var soundMode: String = "off" { // "off", "ping", "loop"
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("sound", soundMode)
        }
    }
    @Published var mixedPort: String = "7891" {
        didSet {
            guard !isHydratingPrefs else { return }
            if let p = UInt16(mixedPort), p > 0 {
                syncPreference("mixed-port", "\(p)")
            }
        }
    }
    @Published var allowLan: Bool = false {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("allow-lan", allowLan ? "true" : "false")
        }
    }
    @Published var ipv6: Bool = true {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("ipv6", ipv6 ? "true" : "false")
        }
    }
    @Published var dnsServer: String = "1.1.1.1" {
        didSet {
            guard !isHydratingPrefs else { return }
            syncPreference("dns", dnsServer)
        }
    }

    private func syncPreference(_ key: String, _ value: String) {
        if PurrSnapshot.active { return }
        key.withCString { kPtr in
            value.withCString { vPtr in
                _ = purr_core_update_preference(kPtr, vPtr)
            }
        }
    }

    private func loadPersistedPreferences() {
        guard let rawPtr = purr_core_load_preferences_json() else { return }
        defer { purr_string_free(rawPtr) }
        let jsonStr = String(cString: rawPtr)
        guard let data = jsonStr.data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return
        }
        isHydratingPrefs = true
        defer { isHydratingPrefs = false }
        if let v = dict["auto_wake"] as? Bool { autoWake = v }
        if let v = dict["auto_swap"] as? Bool { autoSwap = v }
        if let v = dict["block_ads"] as? Bool { blockAds = v }
        if let v = dict["close_to_tray"] as? Bool { closeAction = v ? "tray" : "quit" }
        if let v = dict["system_proxy"] as? Bool { sysProxy = v }
        if let v = dict["mixed_port"] as? Int, v > 0 { mixedPort = "\(v)" }
        if let v = dict["allow_lan"] as? Bool { allowLan = v }
        if let v = dict["ipv6"] as? Bool { ipv6 = v }
        if let v = dict["dns"] as? String, !v.isEmpty { dnsServer = v }
        if let v = dict["sound"] as? String, !v.isEmpty { soundMode = v }
    }

    func copyTerminalProxyCommand() {
        let port = Int(mixedPort) ?? 7891
        let cmd = "export https_proxy=http://127.0.0.1:\(port) http_proxy=http://127.0.0.1:\(port) all_proxy=socks5://127.0.0.1:\(port)"
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(cmd, forType: .string)
        showToast("已复制终端代理命令 (127.0.0.1:\(port))")
        plog("info", "已复制终端环境变量 → 127.0.0.1:\(port)")
    }

    func flushDnsCache() {
        _ = purr_core_flush_dns()
        showToast("系统 DNS 缓存已刷新")
        plog("info", "已刷新 macOS 系统 DNS 缓存 (dscacheutil)")
    }

    func checkForUpdates() {
        showToast("正在与远端核对版本…")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let ptr = purr_core_check_update_json() else { return }
            defer { purr_string_free(ptr) }
            let str = String(cString: ptr)
            guard let data = str.data(using: .utf8),
                  let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                return
            }
            let ok = dict["ok"] as? Bool ?? false
            let hasUpdate = dict["has_update"] as? Bool ?? false
            let latest = dict["latest"] as? String ?? ""
            let current = dict["current"] as? String ?? PurrStore.appVersion
            let urlStr = dict["url"] as? String ?? ""
            let err = dict["error"] as? String ?? ""
            DispatchQueue.main.async {
                guard let self = self else { return }
                if !ok {
                    // The check did not get an answer: say so, never "已是最新".
                    self.showToast("检查更新失败" + (err.isEmpty ? "" : "：\(err)"))
                    self.plog("warn", "检查更新失败" + (err.isEmpty ? "" : "：\(err)"))
                } else if hasUpdate {
                    self.showToast("发现新版本 v\(latest) · 正在打开下载页")
                    if let url = URL(string: urlStr) {
                        NSWorkspace.shared.open(url)
                    }
                } else {
                    self.showToast("当前已是最新版本 PURR v\(current)")
                }
            }
        }
    }

    /// macOS only lets a root-owned core create the utun device. The Rust side owns the check
    /// (authorised setuid copy in ~/Library/Application Support/Purr/core, byte-identical to the
    /// bundled core); refreshed after authorisation.
    @Published var tunAuthorized: Bool = PurrSnapshot.active ? false : purr_core_tun_authorized()
    @Published var tunAuthorizing: Bool = false

    static let tunUnauthorizedText = "虚拟网卡（TUN）需要管理员授权：点一下会弹出系统密码框，授权后内核以管理员身份运行并开启虚拟网卡。只需授权一次，应用更新换了内核时会再问一次。"

    /// First LAN IPv4 address (en*/bridge*), for the "allow LAN" hint.
    static func lanIPv4() -> String? {
        var ifaddr: UnsafeMutablePointer<ifaddrs>? = nil
        guard getifaddrs(&ifaddr) == 0, let first = ifaddr else { return nil }
        defer { freeifaddrs(ifaddr) }
        var p: UnsafeMutablePointer<ifaddrs>? = first
        while let cur = p {
            let flags = Int32(cur.pointee.ifa_flags)
            if let sa = cur.pointee.ifa_addr, sa.pointee.sa_family == UInt8(AF_INET),
               (flags & IFF_UP) != 0, (flags & IFF_LOOPBACK) == 0 {
                let name = String(cString: cur.pointee.ifa_name)
                if name.hasPrefix("en") || name.hasPrefix("bridge") {
                    var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                    if getnameinfo(sa, socklen_t(sa.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0 {
                        return String(cString: host)
                    }
                }
            }
            p = cur.pointee.ifa_next
        }
        return nil
    }

    /// Ask for the admin password (system dialog) and install a root-owned setuid copy of the bundled
    /// core, then restart the core from it and turn TUN on. Cancel leaves everything as it was.
    func authorizeTunCore() {
        guard !tunAuthorizing else { return }
        #if arch(arm64)
        let coreName = "mihomo-darwin-arm64"
        #else
        let coreName = "mihomo-darwin-amd64"
        #endif
        guard let bundled = Bundle.main.path(forResource: coreName, ofType: nil) else {
            showToast("找不到内核文件，没法授权")
            return
        }
        let dir = NSHomeDirectory() + "/Library/Application Support/Purr/core"
        let dst = dir + "/" + coreName
        func q(_ p: String) -> String { "'" + p.replacingOccurrences(of: "'", with: "'\\''") + "'" }
        let sh = "mkdir -p \(q(dir)) && rm -f \(q(dst)) && cp \(q(bundled)) \(q(dst)) && chown root:wheel \(q(dst)) && chmod 4755 \(q(dst))"
        let esc = sh.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"")
        let script = "do shell script \"\(esc)\" with prompt \"PURR 需要授权内核来开启虚拟网卡（TUN）。\" with administrator privileges"
        tunAuthorizing = true
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let p = Process()
            p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
            p.arguments = ["-e", script]
            let err = Pipe()
            p.standardError = err
            p.standardOutput = Pipe()
            var status: Int32 = -1
            do { try p.run(); p.waitUntilExit(); status = p.terminationStatus } catch {}
            let errText = String(data: err.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            let authorized = status == 0 && purr_core_tun_authorized()
            DispatchQueue.main.async {
                guard let self = self else { return }
                if !authorized {
                    self.tunAuthorizing = false
                    if errText.contains("-128") || errText.contains("取消") || errText.lowercased().contains("cancel") {
                        self.showToast("已取消授权，虚拟网卡保持关闭")
                    } else {
                        self.showToast("授权没有成功，虚拟网卡保持关闭")
                        self.plog("warn", "TUN 授权失败：" + errText.trimmingCharacters(in: .whitespacesAndNewlines))
                    }
                    return
                }
                self.tunAuthorized = true
                self.plog("info", "已授权内核（setuid），正在重启内核")
                self.showToast("已授权，正在重启内核…")
                DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                    let rc = purr_core_restart_core()
                    DispatchQueue.main.async {
                        guard let self = self else { return }
                        self.tunAuthorizing = false
                        if rc == 0 {
                            self.toggleTun()
                        } else {
                            self.showToast("内核重启没成功，先用系统代理")
                        }
                    }
                }
            }
        }
    }

    func toggleTun() {
        let next = !tunEnabled
        if next && !tunAuthorized {
            tunAuthorized = purr_core_tun_authorized()
        }
        if next && !tunAuthorized {
            authorizeTunCore()
            return
        }
        showToast(next ? "正在开启虚拟网卡 (TUN)…" : "正在关闭虚拟网卡 (TUN)…")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let code = purr_core_set_tun(next)
            DispatchQueue.main.async {
                guard let self = self else { return }
                if code == 0 {
                    self.tunEnabled = next
                    if next && !self.isAwake {
                        self.isAwake = true
                    }
                    self.showToast("虚拟网卡" + (next ? " 已开启" : " 已关闭"))
                    self.plog("info", "虚拟网卡 (TUN) " + (next ? "已开启" : "已关闭"))
                } else {
                    self.tunEnabled = false
                    self.showToast("虚拟网卡没有开起来，先用系统代理")
                }
            }
        }
    }

    func selectNode(inGroup groupIdx: Int, nodeId: Int) {
        guard groups.indices.contains(groupIdx), nodes.indices.contains(nodeId) else { return }
        groups[groupIdx].selected = nodeId
        let gName = groups[groupIdx].name
        let nName = nodes[nodeId].name
        gName.withCString { gPtr in
            nName.withCString { nPtr in
                _ = purr_core_select_node(gPtr, nPtr)
            }
        }
        showToast("\(gName) → \(nName)")
        plog("info", "\(gName) 换窝到 \(nName)")
    }

    func closeAllConnections() {
        _ = purr_core_close_all_connections()
        conns.removeAll()
        showToast("已断开全部连接")
        plog("info", "已断开全部活动连接")
    }

    func closeConnection(id: Int) {
        if let idx = conns.firstIndex(where: { $0.id == id }) {
            let host = conns[idx].host
            let rawId = conns[idx].rawId
            conns.remove(at: idx)
            if !rawId.isEmpty {
                rawId.withCString { _ = purr_core_close_connection($0) }
            }
            showToast("已断开 \(host)")
        }
    }

    func activateSubscription(id: Int) {
        guard !isImporting else { return }
        isImporting = true
        showToast("正在切换订阅…")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let ptr = purr_core_use_subscription(UInt64(id)) else { return }
            defer { purr_string_free(ptr) }
            let json = String(cString: ptr)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.isImporting = false
                if let data = json.data(using: .utf8),
                   let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                   dict["ok"] as? Bool == true {
                    let name = dict["name"] as? String ?? "订阅"
                    self.syncSnapshotFromCore()
                    self.showToast("已换用订阅 → \(name)")
                    self.plog("info", "切换订阅为 \(name)")
                } else {
                    self.showToast("切换订阅失败，请先点更新")
                }
            }
        }
    }

    func refreshSubscription(id: Int) {
        guard !isImporting else { return }
        isImporting = true
        showToast("正在更新订阅…")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let ptr = purr_core_update_subscription(UInt64(id)) else { return }
            defer { purr_string_free(ptr) }
            let json = String(cString: ptr)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.isImporting = false
                if let data = json.data(using: .utf8),
                   let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                   dict["ok"] as? Bool == true {
                    let name = dict["name"] as? String ?? "订阅"
                    let count = dict["nodes"] as? Int ?? self.nodes.count
                    self.syncSnapshotFromCore()
                    self.showToast("订阅已更新 · \(name) · \(count) 个窝点")
                    self.plog("info", "已更新订阅 \(name)，共 \(count) 个窝点")
                } else {
                    let err = (json.data(using: .utf8).flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] })?["error"] as? String
                    self.showToast(err ?? "订阅更新失败")
                }
            }
        }
    }

    // Toast
    @Published var toastText: String = ""
    @Published var toastVisible: Bool = false
    private var toastTask: Task<Void, Never>? = nil

    // Service Latency Status & IP Diagnostics Modal
    struct ServiceProbe: Identifiable {
        let id: String
        let name: String
        let domain: String
        var routeGroup: String
        /// 0 = 还没测（显示「—」），-1 = 超时，>0 = 实测毫秒
        var ms: Int
        /// The route is a REJECT rule: blocked, not slow.
        var blocked: Bool = false
        var msText: String { blocked ? "已拦截" : (ms > 0 ? "\(ms)ms" : (ms < 0 ? "超时" : "—")) }
    }
    @Published var showServiceDetail: Bool = false {
        didSet { if showServiceDetail && !oldValue { probeServices() } }
    }
    @Published var isTestingServices: Bool = false
    @Published var serviceProbes: [ServiceProbe] = [
        // Per-site latency is not measured yet: no made-up numbers or route guesses, the UI shows "—".
        ServiceProbe(id: "google", name: "Google", domain: "www.google.com", routeGroup: "", ms: 0),
        ServiceProbe(id: "github", name: "GitHub", domain: "api.github.com", routeGroup: "", ms: 0),
        ServiceProbe(id: "claude", name: "Claude", domain: "claude.ai", routeGroup: "", ms: 0),
        ServiceProbe(id: "openai", name: "ChatGPT", domain: "chatgpt.com", routeGroup: "", ms: 0),
        ServiceProbe(id: "youtube", name: "YouTube", domain: "www.youtube.com", routeGroup: "", ms: 0),
        ServiceProbe(id: "telegram", name: "Telegram", domain: "core.telegram.org", routeGroup: "", ms: 0),
        ServiceProbe(id: "twitter", name: "X / Twitter", domain: "x.com", routeGroup: "", ms: 0),
        ServiceProbe(id: "apple", name: "Apple", domain: "www.apple.com", routeGroup: "", ms: 0)
    ]

    private var lastServiceProbeAt: Date? = nil
    /// 概览页自动重测的心跳：每 5 秒看一眼，真正发测由 probeServices 的 60 秒节流决定。
    private var serviceAutoTimer: Timer? = nil
    /// Mac 整机睡眠中（NSWorkspace willSleep → didWake）。
    private var systemAsleep: Bool = false

    /// 窗口可见、停在概览、猫醒着、电脑没睡时才自动测；任一条件不满足就停掉定时器。
    func updateServiceAutoProbe() {
        let want = !PurrSnapshot.active
            && page == 0
            && isAwake
            && !systemAsleep
            && PurrVisibility.shared.mainWindowVisible
        if want {
            guard serviceAutoTimer == nil else { return }
            probeServices()
            let t = Timer(timeInterval: 5, repeats: true) { [weak self] _ in
                self?.probeServices()
            }
            t.tolerance = 1
            RunLoop.main.add(t, forMode: .common)
            serviceAutoTimer = t
        } else {
            serviceAutoTimer?.invalidate()
            serviceAutoTimer = nil
        }
    }

    private func observeServiceAutoProbeConditions() {
        visibilityCancellable = PurrVisibility.shared.$mainWindowVisible
            .removeDuplicates()
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.updateServiceAutoProbe() }
        let ws = NSWorkspace.shared.notificationCenter
        ws.addObserver(forName: NSWorkspace.willSleepNotification, object: nil, queue: .main) { [weak self] _ in
            self?.systemAsleep = true
            self?.updateServiceAutoProbe()
        }
        ws.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.systemAsleep = false
            // 刚醒网络还没恢复，晚几秒再开
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) { self?.updateServiceAutoProbe() }
        }
    }
    private var visibilityCancellable: AnyCancellable? = nil
    /// 常用网站真测：按每个网站实际走的分流（规则匹配出的组/节点，直连走 DIRECT）
    /// 调 mihomo `/proxies/{组}/delay?url=<网站>&timeout=5000`。自动触发 60 秒节流。
    func probeServices(force: Bool = false) {
        guard !PurrSnapshot.active, !isTestingServices else { return }
        if !force, let t = lastServiceProbeAt, Date().timeIntervalSince(t) < 60 { return }
        isTestingServices = true
        lastServiceProbeAt = Date()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var parsed: [String: (String, Int, Bool)] = [:]
            if let ptr = purr_core_probe_sites_json() {
                let str = String(cString: ptr)
                purr_string_free(ptr)
                if let data = str.data(using: .utf8),
                   let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                   let sites = obj["sites"] as? [[String: Any]] {
                    for site in sites {
                        guard let id = site["id"] as? String else { continue }
                        let group = site["group"] as? String ?? ""
                        let node = site["node"] as? String ?? ""
                        let via = site["via"] as? String ?? (group.isEmpty ? "" : (group == "DIRECT" ? "直连" : (node.isEmpty ? group : "\(group) › \(node)")))
                        let ms = (site["ms"] as? NSNumber)?.intValue ?? -1
                        // 连走哪条路都没查到（内核没就绪）：拿不到就是「—」，不算超时
                        let blocked = group.uppercased().hasPrefix("REJECT")
                        parsed[id] = (via, ms > 0 ? ms : (group.isEmpty || blocked ? 0 : -1), blocked)
                    }
                }
            }
            DispatchQueue.main.async {
                guard let self = self else { return }
                for i in self.serviceProbes.indices {
                    if let r = parsed[self.serviceProbes[i].id] {
                        self.serviceProbes[i].routeGroup = r.0
                        self.serviceProbes[i].ms = r.1
                        self.serviceProbes[i].blocked = r.2
                    }
                }
                // 内核还没就绪（一条路由都没查到）就不计满节流，15 秒后重试
                if !parsed.values.contains(where: { !$0.0.isEmpty }) {
                    self.lastServiceProbeAt = Date().addingTimeInterval(-45)
                }
                self.isTestingServices = false
            }
        }
    }

    func retestServices() {
        probeServices(force: true)
    }

    // Trouble countdown
    @Published var troubleCountdown: Int = 0
    private var troubleTimer: Timer? = nil

    private var tickTimer: Timer? = nil
    /// 30 fps timer that only runs while a hold, flood animation or ping sound is active.
    private var fastTimer: Timer? = nil
    private var pingUntil: TimeInterval = 0
    private let synth = PurrSynth()

    let pageTabs: [(id: String, title: String)] = [
        ("ov", "概览"),
        ("px", "窝点"),
        ("sb", "订阅"),
        ("cn", "连接"),
        ("rl", "规则"),
        ("st", "习惯")
    ]

    init() {
        if PurrSnapshot.active { return }
        if !purr_core_disclaimer_accepted() {
            disclaimerPending = true
            return
        }
        bootCore()
    }

    /// Records the typed confirmation, then starts everything init() held back.
    @discardableResult
    func acceptDisclaimer(_ typed: String) -> Bool {
        guard disclaimerPending else { return true }
        let rc = typed.withCString { purr_core_accept_disclaimer($0) }
        guard rc == 0 else { return false }
        disclaimerPending = false
        bootCore()
        return true
    }

    func rateParts(_ mbPerSec: Double) -> (String, String) {
        statsReady ? PurrRate.parts(mbPerSec) : ("—", "")
    }

    func rateText(_ mbPerSec: Double) -> String {
        let p = rateParts(mbPerSec)
        return p.1.isEmpty ? p.0 : "\(p.0) \(p.1)"
    }

    private func bootCore() {
        _ = purr_core_init()
        loadPersistedPreferences()
        if autoWake {
            isAwake = true
            connectedAt = Date()
        }
        plog("purr", "PURR 内核已就绪")
        syncSnapshotFromCore()
        startTimers()
        observeServiceAutoProbeConditions()
        // 启动时停在概览：等内核就绪后开始自动测常用网站
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
            self?.updateServiceAutoProbe()
        }
    }

    func showToast(_ text: String) {
        toastText = text
        withAnimation(.easeOut(duration: 0.22)) {
            toastVisible = true
        }
        toastTask?.cancel()
        toastTask = Task {
            try? await Task.sleep(nanoseconds: 1_600_000_000)
            if !Task.isCancelled {
                withAnimation(.easeIn(duration: 0.22)) {
                    toastVisible = false
                }
            }
        }
    }

    private static let logTimeFormatter: DateFormatter = {
        let df = DateFormatter()
        df.dateFormat = "HH:mm:ss"
        return df
    }()

    func plog(_ level: String, _ msg: String) {
        let t = Self.logTimeFormatter.string(from: Date())
        logs.insert(PurrLog(time: t, level: level, msg: msg), at: 0)
        if logs.count > 40 {
            logs = Array(logs.prefix(40))
        }
    }

    var currentPrimaryNode: PurrNode {
        guard !groups.isEmpty, !nodes.isEmpty else {
            return PurrNode(id: 0, name: "—", proto: "—", ms: 0, jitter: -1, loss: -1, uptime: -1, history: [])
        }
        let idx = min(max(0, groups[0].selected), nodes.count - 1)
        return nodes[idx]
    }

    func nodeOf(groupName: String) -> String {
        if let g = groups.first(where: { $0.name == groupName }), nodes.indices.contains(g.selected) {
            return nodes[g.selected].name
        }
        return currentPrimaryNode.name
    }

    var modeLabel: String {
        switch mode {
        case "global": return "全局"
        case "direct": return "直连"
        default: return "分流"
        }
    }

    /// Local (direct) exit as text for the modal: "检测中…" only while the first lookup runs.
    var directIpText: String { directState == "pending" ? "检测中…" : directIp }
    var directLocText: String { directState == "failed" ? "查不到本地出口（稍后自动重试）" : (directLoc.isEmpty ? "—" : directLoc) }

    func exitInfo() -> (ip: String, loc: String) {
        let dIp = directState == "ok" ? directIp : "—"
        let dLoc = directState == "ok" && !directLoc.isEmpty ? directLoc : "本地出口"
        if !isAwake {
            return (dIp, "\(dLoc)（睡着时未经过窝点）")
        }
        if let e = errKind {
            return (dIp, "本地 IP · 代理异常（\(troubleInfo(for: e).st)）")
        }
        if mode == "direct" {
            return (dIp, "\(dLoc)，没经过窝点")
        }
        let via = groups.isEmpty ? "" : "经由 \(currentPrimaryNode.name) · \(currentPrimaryNode.proto)"
        switch exitState {
        case "ok":
            return (realExitIp, [realExitLoc, via].filter { !$0.isEmpty }.joined(separator: " · "))
        case "failed":
            return ("—", ["出口 IP 查不到", via].filter { !$0.isEmpty }.joined(separator: " · "))
        default:
            return ("查找中…", via)
        }
    }

    // Hold to wake / sleep
    func beginHold() {
        if disclaimerPending { return }
        if firstRun && !isAwake {
            showToast("先给它一个窝 · 导入订阅")
            page = 0
            shakeCount += 1
            return
        }
        if holding { return }
        holding = true
        holdProgress = 0.0
        holdStart = PurrClock.now()
        ensureFastTick()
    }

    func endHold() {
        guard holding else { return }
        holding = false
        holdProgress = 0.0
    }

    func wake() {
        if disclaimerPending { return }
        if soundMode == "ping" {
            pingUntil = PurrClock.now() + 1.5
        }
        let earX = 1200.0 * (Double(page) + 0.5) / Double(pageTabs.count)
        floodX = CGFloat(earX)
        floodY = 110
        floodAnimating = true
        floodTargetAwake = true
        floodStart = PurrClock.now()
        ensureFastTick()
        isAwake = true
        connectedAt = Date()
        elapsedStr = "00:00:00"
        clearHistory()
        let name = groups.isEmpty ? "" : currentPrimaryNode.name
        showToast(name.isEmpty ? "已连接" : "已连接 · \(name)")
        plog("purr", name.isEmpty ? "摸醒了猫" : "摸醒了猫，连接到 \(name)")
    }

    func sleep() {
        if soundMode == "ping" {
            pingUntil = PurrClock.now() + 1.5
        }
        let earX = 1200.0 * (Double(page) + 0.5) / Double(pageTabs.count)
        floodX = CGFloat(earX)
        floodY = 110
        floodAnimating = true
        floodTargetAwake = false
        floodStart = PurrClock.now()
        ensureFastTick()
        isAwake = false
        conns.removeAll()
        dnSpeed = 0
        upSpeed = 0
        load = 0
        // The chart and "60 秒峰值" describe this session only.
        clearHistory()
        showToast("已断开 · 它又睡着了")
        plog("purr", "哄猫睡觉，断开连接")
    }

    func switchMode(_ newMode: String) {
        mode = newMode
        showToast("走法 → \(modeLabel)")
        plog("info", "走法切换为 \(modeLabel)")
    }

    func triggerSpeedTest(quiet: Bool = false) {
        guard !isTestingNodes else { return }
        isTestingNodes = true
        let groupName = groups.indices.contains(selectedGroupIdx) ? groups[selectedGroupIdx].name : ""
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let ptr = groupName.withCString { purr_core_test_group($0) }
            if let ptr = ptr {
                purr_string_free(ptr)
            }
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.syncSnapshotFromCore(recordHistory: true)
                self.isTestingNodes = false
                if !quiet { self.showToast("测速完成") }
            }
        }
    }

    func bestAlternativeNodeId() -> Int {
        guard !groups.isEmpty else { return 0 }
        let cands = groups[0].nodeIds.filter { $0 != badNodeId && nodes.indices.contains($0) }
        return cands.min(by: { nodes[$0].sortMs < nodes[$1].sortMs }) ?? 0
    }

    func troubleInfo(for kind: String) -> (st: String, k: String, h: String, p: String, actions: [(String, String)]) {
        switch kind {
        case "timeout":
            let badName = nodes.indices.contains(badNodeId) ? nodes[badNodeId].name : currentPrimaryNode.name
            let altIdx = bestAlternativeNodeId()
            let b = nodes.indices.contains(altIdx) ? nodes[altIdx] : currentPrimaryNode
            return (
                st: "\(badName) 没回应",
                k: "TROUBLE · 节点超时",
                h: "\(badName) 没回应了",
                p: "连续 3 次超过 5 秒没有回音，已经重试过。问题在窝点这边，你的网络是好的。",
                actions: [("换到 \(b.name) · \(b.msText)", "swap"), ("再试一次", "retry")]
            )
        case "offline":
            return (
                st: "本机没有网络",
                k: "TROUBLE · 断网",
                h: "电脑自己断网了",
                p: "Wi-Fi 没有连上，跟窝点无关。连上网之后它会自己继续呼噜。",
                actions: [("重新检测", "retry"), ("打开网络设置", "net")]
            )
        case "sub":
            let subName = subscriptions.first(where: { $0.active })?.name ?? "当前订阅"
            return (
                st: "订阅已过期",
                k: "TROUBLE · 订阅过期",
                h: "\(subName) 过期了",
                p: "窝点已停用。续费后点 更新，或者先换用备用订阅。",
                actions: [("去续费 / 更新", "subpg"), ("换用备用订阅", "backup")]
            )
        default: // "port"
            return (
                st: "端口 \(mixedPort) 被占用",
                k: "TROUBLE · 端口被占",
                h: "\(mixedPort) 被别人占了",
                p: "其他程序正在用这个端口，两只猫抢一个窝。换个端口，或者先退出冲突程序。",
                actions: [("换成 7891", "port"), ("去 习惯 里改", "stpg")]
            )
        }
    }

    func clearHistory() {
        histDn = Array(repeating: 0.0, count: PurrStore.historySamples)
        histUp = Array(repeating: 0.0, count: PurrStore.historySamples)
        histCount = 0
        peakDn = 0
        peakUp = 0
    }

    func setErr(_ k: String?) {
        troubleTimer?.invalidate()
        troubleTimer = nil
        troubleCountdown = 0
        errKind = k
        if let k = k {
            // Snapshot fixtures show trouble states awake; live trouble comes from the core and
            // never wakes the cat by itself.
            if PurrSnapshot.active && !isAwake {
                isAwake = true
                connectedAt = Date()
            }
            badNodeId = groups.first?.selected ?? 0
            if k == "timeout" && autoSwap {
                troubleCountdown = 8
                troubleTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
                    DispatchQueue.main.async {
                        guard let self = self else { return }
                        self.troubleCountdown -= 1
                        if self.troubleCountdown <= 0 {
                            self.troubleTimer?.invalidate()
                            self.troubleTimer = nil
                            self.performTroubleFix("swap")
                        }
                    }
                }
            }
        }
    }

    func performTroubleFix(_ action: String) {
        let prevErr = errKind
        switch action {
        case "swap", "retry":
            if PurrSnapshot.active {
                setErr(nil)
                return
            }
            // Real fix in the core: "swap" probes the other members and switches to the fastest,
            // "retry" re-checks (delay probe / network) and only clears the trouble if it passes.
            showToast(action == "swap" ? "正在找个暖和的窝…" : "闻一闻…")
            troubleTimer?.invalidate()
            troubleTimer = nil
            troubleCountdown = 0
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                let ptr = action.withCString { purr_core_fix_trouble_json($0) }
                var ok = false
                var msg = ""
                if let ptr = ptr {
                    let str = String(cString: ptr)
                    purr_string_free(ptr)
                    if let data = str.data(using: .utf8),
                       let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                        ok = obj["ok"] as? Bool ?? false
                        msg = obj["message"] as? String ?? ""
                    }
                }
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    self.showToast(msg.isEmpty ? (ok ? "继续呼噜" : "还没好") : msg)
                    self.plog(ok ? "info" : "warn", msg)
                    self.syncSnapshotFromCore()
                    if ok && self.errKind == prevErr { self.setErr(nil) }
                }
            }
            return
        case "subpg":
            page = 2
        case "stpg":
            page = 5
        case "net":
            if let url = URL(string: "x-apple.systempreferences:com.apple.Network-Settings.extension") {
                NSWorkspace.shared.open(url)
            }
            showToast("打开系统网络设置")
        case "backup":
            if let alt = subscriptions.first(where: { !$0.active }) {
                activateSubscription(id: alt.id)
                setErr(nil)
            } else {
                page = 2
            }
        case "port":
            mixedPort = "7891"
            setErr(nil)
            showToast("混合端口 → 7891")
            plog("info", "混合端口改为 7891")
        default:
            break
        }
        demoState = errKind ?? "ok"
    }

    func performImport(overrideUrl: String? = nil) {
        let candidate = (overrideUrl ?? (page == 2 ? subInputUrl : firstRunUrl)).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !isImporting else { return }
        if candidate.isEmpty {
            if let activeSub = subscriptions.first(where: { $0.active }) {
                refreshSubscription(id: activeSub.id)
                return
            }
            showToast("请先粘贴订阅链接 (https://…)")
            return
        }
        isImporting = true
        showToast("正在拉取并校验订阅…")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let ptr = candidate.withCString({ purr_core_import_subscription($0) }) else { return }
            defer { purr_string_free(ptr) }
            let json = String(cString: ptr)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.isImporting = false
                guard let data = json.data(using: .utf8),
                      let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    self.showToast("订阅导入失败")
                    return
                }
                if dict["ok"] as? Bool == true {
                    let name = dict["name"] as? String ?? "新订阅"
                    let count = dict["nodes"] as? Int ?? 0
                    self.subInputUrl = ""
                    self.firstRunUrl = ""
                    self.firstRun = false
                    self.page = 0
                    self.syncSnapshotFromCore()
                    self.showToast("导入成功 · \(name) · \(count) 个窝点")
                    self.plog("info", "导入订阅 \(name)，共 \(count) 个窝点")
                    self.nudgeTip = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.6) {
                        self.nudgeTip = false
                    }
                    self.triggerSpeedTest()
                } else {
                    let err = dict["error"] as? String ?? "订阅下载失败，请检查链接与网络"
                    self.showToast(err)
                }
            }
        }
    }

    private func syncSnapshotFromCore(recordHistory: Bool = false) {
        guard let ptr = purr_core_snapshot_json() else { return }
        let jsonStr = String(cString: ptr)
        purr_string_free(ptr)
        applySnapshot(jsonStr, recordHistory: recordHistory, sampleTraffic: true)
    }

    /// Applies one core snapshot (`purr_core_snapshot_json`). Every value shown comes from it;
    /// a missing value stays nil and the UI shows "—".
    func applySnapshot(_ jsonStr: String, recordHistory: Bool = false, sampleTraffic: Bool = false) {
        guard let data = jsonStr.data(using: .utf8),
              let snap = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return
        }

        assign(\.memMB, snap["mem_mb"] as? Double)
        assign(\.memShellMB, snap["mem_shell_mb"] as? Double)
        assign(\.memCoreMB, snap["mem_core_mb"] as? Double)
        assign(\.cpuPct, snap["cpu_pct"] as? Double)
        assign(\.realExitIp, snap["exit_ip"] as? String ?? "—")
        assign(\.realExitLoc, snap["exit_loc"] as? String ?? "")
        assign(\.exitState, snap["exit_state"] as? String ?? "pending")
        assign(\.directIp, snap["direct_ip"] as? String ?? "—")
        assign(\.directLoc, snap["direct_loc"] as? String ?? "")
        assign(\.directState, snap["direct_state"] as? String ?? "pending")
        if let tun = snap["tun"] as? Bool {
            assign(\.tunEnabled, tun)
        }
        assign(\.todayTotal, snap["today_total"] as? String)
        assign(\.todayDown, snap["today_down"] as? String)
        assign(\.todayUp, snap["today_up"] as? String)
        assign(\.routeProxy, snap["route_proxy"] as? String)
        assign(\.routeDirect, snap["route_direct"] as? String)
        assign(\.routeProxyFraction, snap["route_proxy_fraction"] as? Double)
        assign(\.statsReady, snap["ready"] as? Bool ?? false)
        assign(\.blockedMice, snap["blocked_count"] as? Int)

        // Trouble detected by the core (same rules as Windows): drive the existing trouble UI.
        let coreTrouble = snap["trouble"] as? String ?? ""
        let next: String? = coreTrouble.isEmpty ? nil : coreTrouble
        if next != self.errKind && !PurrSnapshot.active {
            self.setErr(next)
            if let k = next { self.plog("warn", "卡住了：\(self.troubleInfo(for: k).st)") }
        }

        if isAwake {
            let dn = snap["dn_mb_s"] as? Double ?? 0.0
            let up = snap["up_mb_s"] as? Double ?? 0.0
            assign(\.dnSpeed, dn)
            assign(\.upSpeed, up)
            assign(\.load, min(1.0, dn / 16.0))
            let secs = max(0, Int(Date().timeIntervalSince(connectedAt)))
            assign(\.elapsedStr, String(format: "%02d:%02d:%02d", secs / 3600, (secs / 60) % 60, secs % 60))
            if sampleTraffic && self.statsReady {
                let n = PurrStore.historySamples
                // Build each ring once and publish it once (bounded to historySamples).
                var d = self.histDn
                d.append(dn)
                if d.count > n { d.removeFirst(d.count - n) }
                var u = self.histUp
                u.append(up)
                if u.count > n { u.removeFirst(u.count - n) }
                self.histDn = d
                self.histUp = u
                assign(\.histCount, min(n, self.histCount + 1))
                assign(\.peakDn, self.histDn.suffix(self.histCount).max() ?? 0.0)
                assign(\.peakUp, self.histUp.suffix(self.histCount).max() ?? 0.0)
            }
        }

        // Subscriptions
        if let rawSubs = snap["subscriptions"] as? [[String: Any]] {
            let builtSubs: [PurrSubItem] = rawSubs.compactMap { s in
                guard let id = s["id"] as? Int,
                      let name = s["name"] as? String else { return nil }
                return PurrSubItem(
                    id: id,
                    name: name,
                    desc: s["desc"] as? String ?? "",
                    used: (s["used"] as? NSNumber)?.doubleValue ?? -1,
                    total: (s["total"] as? NSNumber)?.doubleValue ?? 0,
                    exp: s["exp"] as? String ?? "—",
                    active: s["active"] as? Bool ?? false,
                    daysLeft: s["days_left"] as? Int
                )
            }
            assign(\.subscriptions, builtSubs)
        }

        // Groups & Nodes
        if let rawGroups = snap["groups"] as? [[String: Any]], !rawGroups.isEmpty {
            var nodeIndexByName: [String: Int] = [:]
            var builtNodes: [PurrNode] = []
            var builtGroups: [PurrGroup] = []

            for (gIdx, gDict) in rawGroups.enumerated() {
                guard let gName = gDict["name"] as? String, !gName.isEmpty else { continue }
                let gNow = gDict["now"] as? String ?? ""
                let rawNodes = gDict["nodes"] as? [[String: Any]] ?? []
                var nodeIds: [Int] = []
                var selectedNodeId: Int = 0

                for nDict in rawNodes {
                    guard let nName = nDict["name"] as? String, !nName.isEmpty else { continue }
                    let nProto = nDict["proto"] as? String ?? "—"
                    let nMs = max(0, nDict["ms"] as? Int ?? 0)
                    let nodeId: Int
                    if let existing = nodeIndexByName[nName] {
                        nodeId = existing
                    } else {
                        nodeId = builtNodes.count
                        nodeIndexByName[nName] = nodeId
                        var hist = nodeHistories[nName] ?? []
                        if recordHistory {
                            hist.append(nMs > 0 ? nMs : nil)
                            if hist.count > 18 { hist.removeFirst() }
                            nodeHistories[nName] = hist
                        }
                        let valid = hist.compactMap { $0 }
                        let avg = valid.isEmpty ? nMs : (valid.reduce(0, +) / valid.count)
                        let jitter = valid.count < 2 ? -1 : valid.map { abs($0 - avg) }.reduce(0, +) / valid.count
                        // loss / success rate come only from this session's real probes (nil = no answer)
                        let loss = hist.isEmpty ? -1 : Int((Double(hist.count - valid.count) / Double(hist.count) * 100).rounded())
                        let uptime = hist.isEmpty ? -1.0 : Double(valid.count) / Double(hist.count) * 100
                        builtNodes.append(
                            PurrNode(
                                id: nodeId,
                                name: nName,
                                proto: nProto,
                                ms: nMs,
                                jitter: jitter,
                                loss: loss,
                                uptime: uptime,
                                history: hist
                            )
                        )
                    }
                    nodeIds.append(nodeId)
                    if nName == gNow {
                        selectedNodeId = nodeId
                    }
                }
                if !nodeIds.isEmpty {
                    if !nodeIds.contains(selectedNodeId) {
                        selectedNodeId = nodeIds[0]
                    }
                    builtGroups.append(
                        PurrGroup(
                            id: gIdx,
                            name: gName,
                            selected: selectedNodeId,
                            nodeIds: nodeIds
                        )
                    )
                }
            }

            if !builtNodes.isEmpty && !builtGroups.isEmpty {
                assign(\.nodes, builtNodes)
                assign(\.groups, builtGroups)
                if self.selectedGroupIdx >= builtGroups.count {
                    assign(\.selectedGroupIdx, 0)
                }
                assign(\.firstRun, false)
            } else if self.subscriptions.isEmpty {
                assign(\.firstRun, true)
            }
        } else if self.subscriptions.isEmpty {
            assign(\.firstRun, true)
        }

        // Connections
        if let rawConns = snap["connections"] as? [[String: Any]] {
            let builtConns: [PurrConn] = rawConns.enumerated().map { idx, c in
                PurrConn(
                    id: idx + 1,
                    rawId: c["id"] as? String ?? "",
                    app: c["app"] as? String ?? "—",
                    host: c["host"] as? String ?? "—",
                    rule: c["rule"] as? String ?? "—",
                    pol: c["pol"] as? String ?? "—",
                    chain: c["chain"] as? String ?? "",
                    dn: c["dn_kb"] as? Double ?? 0.0,
                    up: c["up_kb"] as? Double ?? 0.0,
                    age: (c["age_s"] as? NSNumber)?.doubleValue,
                    ageText: c["age"] as? String ?? ""
                )
            }
            assign(\.conns, builtConns)
        }

        // Rules
        if let rawRules = snap["rules"] as? [[String: Any]], !rawRules.isEmpty {
            let builtRules: [PurrRuleItem] = rawRules.enumerated().map { idx, r in
                PurrRuleItem(
                    id: idx + 1,
                    kind: r["kind"] as? String ?? "—",
                    match: r["match_str"] as? String ?? "—",
                    policy: r["policy"] as? String ?? "—",
                    hits: (r["hits"] as? NSNumber)?.intValue
                )
            }
            assign(\.allRules, builtRules)
        }
    }

    /// Writes a published value only when it changed. Every @Published write fires
    /// objectWillChange, so writing ~30 unchanged values each second re-rendered the whole window.
    @inline(__always)
    private func assign<T: Equatable>(_ kp: ReferenceWritableKeyPath<PurrStore, T>, _ value: T) {
        if self[keyPath: kp] != value { self[keyPath: kp] = value }
    }

    private var lastSyncSec: TimeInterval = 0

    private func startTimers() {
        // Idle baseline: 1 Hz is enough for the snapshot sync and the tray title.
        tickTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.stepFrame()
        }
        tickTimer?.tolerance = 0.2
    }

    private func ensureFastTick() {
        guard fastTimer == nil else { return }
        fastTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
            self?.stepFrame()
        }
    }

    private func stopFastTickIfIdle(now: TimeInterval) {
        guard fastTimer != nil, !floodAnimating, !holding, now >= pingUntil else { return }
        fastTimer?.invalidate()
        fastTimer = nil
    }

    private func stepFrame() {
        let now = PurrClock.now()
        if floodAnimating {
            let p = min(1.0, (now - floodStart) / 0.55)
            let eased = p * p * (3.0 - 2.0 * p)
            floodRadius = CGFloat(floodTargetAwake ? eased : (1.0 - eased))
            if p >= 1.0 {
                floodAnimating = false
            }
        }
        if holding {
            let dur = isAwake ? 1.6 : 1.0
            holdProgress = min(1.0, (now - holdStart) / dur)
            if holdProgress >= 1.0 {
                holding = false
                holdProgress = 0.0
                if isAwake { sleep() } else { wake() }
            }
        }

        let shouldPurr = (isAwake && soundMode == "loop") || (now < pingUntil)
        if shouldPurr != (synth.targetGain > 0) {
            synth.update(gain: shouldPurr ? 0.35 : 0.0)
        }

        if now - lastSyncSec >= 1.0 {
            lastSyncSec = now
            syncSnapshotFromCore()
            onTrayUpdate?()
        }
        stopFastTickIfIdle(now: now)
    }
}

/// Whether the main window is actually on screen (not covered, minimised or in the tray).
/// Decorative TimelineViews pause when it is false so a hidden window costs no CPU.
final class PurrVisibility: ObservableObject {
    static let shared = PurrVisibility()
    @Published var mainWindowVisible: Bool = true
}
