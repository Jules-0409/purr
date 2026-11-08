import SwiftUI
import AppKit

/// First-launch notice. Shown before the core, system proxy or TUN are touched; the store only
/// starts them after `purr_core_accept_disclaimer` has saved the acceptance.
enum PurrDisclaimer {
    static let title = "使用前请阅读"
    static let body = "Purr 只是一个网络代理客户端工具，不提供任何节点、订阅或网络服务。请遵守你所在地的法律法规，合法使用。因使用本软件产生的一切后果与风险，由使用者自行承担。"
    static let phrase = "我已阅读并同意"
    static let size = CGSize(width: 560, height: 320)

    static func matches(_ input: String) -> Bool {
        input.trimmingCharacters(in: .whitespacesAndNewlines) == phrase
    }
}

/// Borderless windows refuse key status by default, which would leave the text field untypable.
final class PurrKeyWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

struct PurrDisclaimerView: View {
    @Binding var input: String
    var failed: Bool = false
    let onAccept: () -> Void
    let onQuit: () -> Void
    @FocusState private var focused: Bool

    private var theme: PurrTheme {
        PurrTheme.current(isAwake: true, mode: "rule", hasErr: false, isDark: false)
    }
    private var ok: Bool { PurrDisclaimer.matches(input) }

    var body: some View {
        let t = theme
        VStack(alignment: .leading, spacing: 0) {
            Text("PURR · 免责声明")
                .font(.system(size: 11, design: .monospaced))
                .tracking(1.8)
                .foregroundColor(t.mute)
            Text(PurrDisclaimer.title)
                .font(.system(size: 24, weight: .black))
                .foregroundColor(t.ink)
                .padding(.top, 10)
            Text(PurrDisclaimer.body)
                .font(.system(size: 13.5))
                .lineSpacing(5)
                .foregroundColor(t.ink)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 14)
            (Text("请在下方输入「")
                + Text(PurrDisclaimer.phrase).fontWeight(.bold)
                + Text("」后继续。"))
                .font(.system(size: 12.5))
                .foregroundColor(t.mute)
                .padding(.top, 16)
            HStack(spacing: 8) {
                TextField("", text: $input, prompt: Text("在此输入").foregroundColor(t.mute))
                    .textFieldStyle(.plain)
                    .font(.system(size: 13, design: .monospaced))
                    .foregroundColor(t.ink)
                    .focused($focused)
                    .onSubmit { if ok { onAccept() } }
                Text(ok ? "✓" : "")
                    .font(.system(size: 13, weight: .bold, design: .monospaced))
                    .foregroundColor(t.ink)
                    .frame(width: 14)
            }
            .padding(.vertical, 6)
            .overlay(alignment: .bottom) { Rectangle().fill(t.ink).frame(height: 1.5) }
            .padding(.top, 10)
            if failed {
                Text("保存失败，请重试")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(t.ink)
                    .padding(.top, 6)
            }
            Spacer(minLength: 0)
            HStack(spacing: 14) {
                Spacer()
                DisclaimerBracket(label: "退出", enabled: true, theme: t, action: onQuit)
                DisclaimerBracket(label: "同意", enabled: ok, theme: t, action: onAccept)
            }
        }
        .padding(.horizontal, 36)
        .padding(.top, 30)
        .padding(.bottom, 24)
        .frame(width: PurrDisclaimer.size.width, height: PurrDisclaimer.size.height, alignment: .topLeading)
        .background(t.bg)
        .onAppear { focused = true }
    }
}

/// Same look as PurrBracketButton, with a disabled state.
private struct DisclaimerBracket: View {
    let label: String
    let enabled: Bool
    let theme: PurrTheme
    let action: () -> Void
    @State private var hover = false

    var body: some View {
        let lit = hover && enabled
        Button(action: { if enabled { action() } }) {
            HStack(spacing: 5) {
                Text("[")
                Text(label)
                Text("]")
            }
            .font(.system(size: 13, weight: .semibold, design: .monospaced))
            .tracking(0.5)
            .padding(.horizontal, 5)
            .padding(.vertical, 4)
            .background(lit ? theme.ink : Color.clear)
            .foregroundColor(lit ? theme.bg : theme.ink)
            .clipShape(RoundedRectangle(cornerRadius: 3))
            .opacity(enabled ? 1 : 0.35)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .onHover { hover = $0 }
    }
}
