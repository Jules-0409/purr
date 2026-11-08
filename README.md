# PURR

A native Rust + Slint desktop client for Mihomo, based on the PURR v5 desktop prototype. Six pages: overview, nodes, subscriptions, connections/logs, rules and preferences. No webview.

Current status: the supplied Slint frontend is integrated with the real backend. Combined RSS still exceeds 150 MB on macOS; Windows runtime validation is pending.

## Download (下载)

Intro page: **[liujufu.dev/purr](https://liujufu.dev/purr/)**

| Platform | File |
| --- | --- |
| Windows 10/11 x64 installer | [Purr-Windows-x64-Setup.exe](https://liujufu.dev/purr/downloads/Purr-Windows-x64-Setup.exe) |
| Windows x64 portable | [Purr-Windows-x64.zip](https://liujufu.dev/purr/downloads/Purr-Windows-x64.zip) |
| macOS 14+ (Apple silicon) | [Purr-macOS-arm64.dmg](https://liujufu.dev/purr/downloads/Purr-macOS-arm64.dmg) |
| macOS 14+ (Apple silicon) portable | [Purr-macOS-arm64.zip](https://liujufu.dev/purr/downloads/Purr-macOS-arm64.zip) |

Checksums: [SHA256SUMS.txt](https://liujufu.dev/purr/downloads/SHA256SUMS.txt)

The macOS app is ad-hoc signed and not notarized. If macOS blocks the first open, allow it in **System Settings → Privacy & Security** (系统设置 → 隐私与安全性) and choose **Open Anyway**. On first launch PURR shows a usage notice; type **我已阅读并同意** to continue.

## Windows

Use the x64 installer, or extract the entire portable ZIP and run `Purr.exe`. Keep its `resources` folder beside the executable. The package includes a SHA-256-verified Mihomo v1.19.33 x64 compatible build. The app statically links the Microsoft C runtime.

Paste a Clash/Mihomo YAML subscription URL into the subscription page and choose **导入**. Hold the cat ears or Space for one second to connect; hold for 1.6 seconds to disconnect. System proxy and TUN can be controlled separately. TUN requires administrator privileges and kernel/driver support. Closing the window leaves PURR in the tray; choose **退出** to stop the core and restore the previous system proxy.

Multiple subscriptions retain separate cached profiles, provider titles, usage and expiry. Update, edit or switch them from the subscription page. Editing a URL keeps the same subscription entry. Drop a UTF-8 `.yaml` or `.yml` profile onto the window to import a local file; profiles are limited to 2 MB.

Preferences, the active profile and import backups are under `%APPDATA%\Purr`. The installer runs for the current user and preserves these files during uninstall. Builds are unsigned.

## Development

Use a current stable Rust toolchain. A platform Mihomo binary is required in `resources/` beside the executable (or in the project resource directory for development). Windows expects `resources/mihomo-windows-amd64.exe`.

Third-party binaries and rule data are not stored in this repository. For a local build, put them in `resources/`: a Mihomo binary from [MetaCubeX/mihomo releases](https://github.com/MetaCubeX/mihomo/releases) (`mihomo-darwin-arm64` on macOS; `tools/package_windows.py` downloads and SHA-256-verifies the Windows build itself), `GeoIP.dat` and `GeoSite.dat` from [MetaCubeX/meta-rules-dat releases](https://github.com/MetaCubeX/meta-rules-dat/releases), and, for Windows TUN, the amd64 `wintun.dll` from [wintun.net](https://www.wintun.net/).

The macOS app is SwiftUI in `macos/` and links the Rust core as a static library: run `cargo build --release --lib`, then `swift build -c release` inside `macos/`.

```sh
cargo run --release
cargo run --release -- --preview
cargo run --release -- --preview --state ok --page 1
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Preview mode uses synthetic data and never starts a core or changes system proxy settings. Available states: `sleep`, `ok`, `first`, `timeout`, `offline`, `sub`, `port`, `global`, `direct`, `tray`, `dark`.

```sh
target/release/Purr --preview --capture /absolute/path/screenshots
target/release/Purr --preview --benchmark /absolute/path/memory.json
python3 tools/check_runtime.py
python3 tools/check_probes.py
python3 tools/check_switch.py --binary target/release/Purr
python3 tools/gen_icons.py
```

The runtime check launches the actual GUI/core with a temporary profile and isolated ports, leaves system proxy disabled, transfers local data concurrently, validates real profile transactions and tests core restart. It exits with code 2 when combined RSS exceeds 150 decimal MB. On Windows pass `--binary` pointing to the packaged executable.

`PURR_CONFIG_DIR` selects an absolute configuration directory. `PURR_CONTROLLER_ADDR` accepts a loopback socket address for isolated development checks. Both platforms default to software rendering. GPU support is optional: build with `cargo build --release --features gpu-renderer`, then set `PURR_RENDERER=femtovg`. The default build does not link the optional OpenGL renderer.

Left or right click on the OS tray icon opens the designed Slint popover. The six page navigation buttons remain below the ear band in the main window.

## Windows builds

On a Windows machine with the MSVC Rust toolchain and NSIS:

```powershell
$env:RUSTFLAGS="-C target-feature=+crt-static"
cargo build --locked --release
python tools/package_windows.py --binary target/release/Purr.exe --installer
```

On macOS, `cargo-xwin` can cross-compile with `RUSTFLAGS='-C target-feature=+crt-static' cargo xwin build --release --target x86_64-pc-windows-msvc`. Ensure `RUSTC` points to the toolchain that contains the Windows target. Then run `python3 tools/package_windows.py --installer`. Use `--output-dir` to select a new release directory.

The Windows workflow performs native compilation, strict checks, screenshot/interaction smoke checks, real backend verification and combined memory measurement. It has been added to the repository; local cross-compilation alone does not verify Windows runtime behavior.

GPL-3.0-only. See `LICENSE` and the packaged `SOURCES.txt`.

The frontend uses the supplied Slint pages and controls, connected to the real backend. See [frontend integration](docs/frontend.md).

See [the verification record](docs/verification.md) for measured memory, exercised behavior and remaining validation limits.
