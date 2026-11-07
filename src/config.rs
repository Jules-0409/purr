// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};

pub const MIXED_PORT: u16 = 7891;
static ACTIVE_PORT: AtomicU16 = AtomicU16::new(MIXED_PORT);
pub fn mixed_port() -> u16 {
    ACTIVE_PORT.load(Ordering::Relaxed)
}
pub fn set_mixed_port(port: u16) {
    ACTIVE_PORT.store(port, Ordering::Relaxed);
}
pub const CONTROLLER_ADDR: &str = "127.0.0.1:9097";
pub fn controller_addr() -> String {
    std::env::var("PURR_CONTROLLER_ADDR")
        .ok()
        .filter(|v| {
            v.parse::<std::net::SocketAddr>()
                .is_ok_and(|a| a.ip().is_loopback())
        })
        .unwrap_or_else(|| CONTROLLER_ADDR.into())
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub config_dir: PathBuf,
    pub core_binary: PathBuf,
    pub secret: String,
}

pub fn prepare() -> Result<Settings, String> {
    let config_dir = app_config_dir()?;
    fs::create_dir_all(&config_dir)
        .map_err(|err| format!("create {}: {err}", config_dir.display()))?;
    let config_file = config_dir.join("config.yaml");
    if !config_file.exists() {
        seed_config(&config_file)?;
    }
    let original = fs::read_to_string(&config_file)
        .map_err(|err| format!("read {}: {err}", config_file.display()))?;
    let mut secret = top_level_value(&original, "secret").unwrap_or_default();
    if secret.is_empty() {
        secret = random_secret();
    }
    let preferences = crate::settings::Preferences::load(&config_dir)?;
    set_mixed_port(preferences.mixed_port);
    let patched = preferences.patch_config(&patch_config(
        &original,
        &secret,
        default_interface().as_deref(),
    ));
    if patched != original {
        fs::write(&config_file, &patched)
            .map_err(|err| format!("write {}: {err}", config_file.display()))?;
    }
    copy_geodata(&config_dir)?;
    let core_binary = launch_binary();
    if !core_binary.is_file() {
        return Err(format!("mihomo binary missing: {}", core_binary.display()));
    }
    Ok(Settings {
        config_dir,
        core_binary,
        secret,
    })
}

pub fn app_config_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("PURR_CONFIG_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("PURR_CONFIG_DIR must be absolute".into());
        }
        return Ok(path);
    }
    #[cfg(target_os = "macos")]
    {
        Ok(home_dir()?.join("Library/Application Support/Purr"))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA is not set".to_string())?;
        Ok(PathBuf::from(appdata).join("Purr"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Ok(home_dir()?.join(".config/Purr"))
    }
}

#[cfg(unix)]
fn home_dir() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| "HOME is not set".to_string())
}

pub fn core_binary() -> PathBuf {
    let name = core_binary_name();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [
                dir.join("resources").join(name),
                dir.join(name),
                dir.parent()
                    .map(|contents| contents.join("Resources").join(name))
                    .unwrap_or_else(|| dir.join("Resources").join(name)),
                dir.parent()
                    .map(|contents| contents.join("Resources").join("resources").join(name))
                    .unwrap_or_else(|| dir.join("Resources").join("resources").join(name)),
            ];
            for path in candidates {
                if path.is_file() {
                    return path;
                }
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(name)
}

/// macOS TUN: a root-owned setuid copy of the bundled core in `<config>/core/`, made once with the
/// user's admin password (Swift `authorizeTunCore`). Used only while it is byte-identical to the
/// bundled core, so an app update with a new core asks for authorisation again.
#[cfg(target_os = "macos")]
pub fn authorized_core() -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let path = app_config_dir().ok()?.join("core").join(core_binary_name());
    let meta = fs::metadata(&path).ok()?;
    if meta.uid() != 0 || meta.mode() & 0o4000 == 0 {
        return None;
    }
    let bundled = core_binary();
    if fs::metadata(&bundled).ok()?.len() != meta.len() {
        return None;
    }
    if fs::read(&path).ok()? != fs::read(&bundled).ok()? {
        return None;
    }
    Some(path)
}

#[cfg(not(target_os = "macos"))]
pub fn authorized_core() -> Option<PathBuf> {
    None
}

/// The binary the core is started from: the authorised copy when there is one, else the bundled core.
pub fn launch_binary() -> PathBuf {
    authorized_core().unwrap_or_else(core_binary)
}

pub fn core_binary_name() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "mihomo-darwin-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "mihomo-darwin-amd64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "mihomo-windows-arm64.exe"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "mihomo-windows-amd64.exe"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "mihomo"
    }
}

fn seed_config(dest: &Path) -> Result<(), String> {
    if let Ok(source) = seed_source() {
        fs::copy(&source, dest)
            .map_err(|err| format!("copy {} to {}: {err}", source.display(), dest.display()))?;
    } else {
        // A fresh installation works without another proxy client or geodata.
        crate::settings::atomic_write(dest, b"mixed-port: 7891\nallow-lan: false\nmode: rule\nlog-level: warning\nipv6: true\nproxies: []\nproxy-groups: []\nrules:\n  - MATCH,DIRECT\n")?;
    }
    Ok(())
}

fn seed_source() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let path = home_dir()?.join("Library/Application Support/com.follow.clash/config.yaml");
        if !path.is_file() {
            return Err(format!("FlClash config not found: {}", path.display()));
        }
        Ok(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("config.yaml is missing; seed it before starting Purr".to_string())
    }
}

/// Rules in the seeded or imported config use GEOSITE/GEOIP. Copy bundled dat files once.
pub fn copy_geodata(dir: &Path) -> Result<(), String> {
    let mut resource_dirs = Vec::new();
    if let Some(core_dir) = core_binary().parent() {
        resource_dirs.push(core_dir.to_path_buf());
        resource_dirs.push(core_dir.join("resources"));
    }
    resource_dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"));
    #[cfg(target_os = "macos")]
    if let Ok(home) = home_dir() {
        resource_dirs.push(home.join("Library/Application Support/com.follow.clash"));
    }

    for (candidates, dest_names) in [
        (
            ["GeoIP.dat", "GEOIP.dat", "geoip.dat"],
            ["GeoIP.dat", "geoip.dat"],
        ),
        (
            ["GeoSite.dat", "GEOSITE.dat", "geosite.dat"],
            ["GeoSite.dat", "geosite.dat"],
        ),
    ] {
        let mut found_source = None;
        'outer: for rdir in &resource_dirs {
            for cand in candidates {
                let p = rdir.join(cand);
                if p.is_file() {
                    found_source = Some(p);
                    break 'outer;
                }
            }
        }
        if let Some(source) = found_source {
            for dest_name in dest_names {
                let dest = dir.join(dest_name);
                if !dest.exists() {
                    let _ = fs::copy(&source, &dest);
                }
            }
        }
    }
    #[cfg(windows)]
    {
        let dest = dir.join("wintun.dll");
        if !dest.exists() {
            for rdir in &resource_dirs {
                let candidate = rdir.join("wintun.dll");
                if candidate.is_file() {
                    let _ = fs::copy(&candidate, &dest);
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn random_secret() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("os random");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn patch_config(text: &str, secret: &str, interface: Option<&str>) -> String {
    let mut saw_port = false;
    let mut saw_controller = false;
    let mut saw_secret = false;
    let mut saw_interface = false;
    let mut in_tun = false;
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let newline = &line[body.len()..];
        let trimmed = body.trim_start();
        let indent = body.len() - trimmed.len();
        if indent == 0 && trimmed.starts_with("tun:") {
            in_tun = true;
        } else if in_tun && !trimmed.is_empty() && indent == 0 && !trimmed.starts_with('#') {
            in_tun = false;
        }
        if is_top_level_key(body, "mixed-port") {
            out.push_str("mixed-port: ");
            out.push_str(&MIXED_PORT.to_string());
            saw_port = true;
        } else if is_top_level_key(body, "external-controller") {
            out.push_str("external-controller: \"");
            out.push_str(&controller_addr());
            out.push('"');
            saw_controller = true;
        } else if is_top_level_key(body, "secret") {
            out.push_str("secret: \"");
            out.push_str(secret);
            out.push('"');
            saw_secret = true;
        } else if is_top_level_key(body, "interface-name") {
            saw_interface = true;
            let current = unquote(
                body.split_once(':')
                    .map(|(_, value)| value.trim())
                    .unwrap_or(""),
            );
            if current.is_empty() {
                if let Some(iface) = interface {
                    out.push_str("interface-name: \"");
                    out.push_str(iface);
                    out.push('"');
                } else {
                    out.push_str(body);
                }
            } else {
                out.push_str(body);
            }
        } else if in_tun && trimmed.starts_with("enable:") {
            out.push_str(&" ".repeat(indent));
            out.push_str("enable: false");
        } else {
            out.push_str(body);
        }
        out.push_str(newline);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !saw_port {
        out.push_str(&format!("mixed-port: {MIXED_PORT}\n"));
    }
    if !saw_controller {
        out.push_str(&format!("external-controller: \"{}\"\n", controller_addr()));
    }
    if !saw_secret {
        out.push_str(&format!("secret: \"{secret}\"\n"));
    }
    if !saw_interface {
        if let Some(iface) = interface {
            out.push_str(&format!("interface-name: \"{iface}\"\n"));
        }
    }
    out
}

/// Physical default interface. Skips utun so a running FlClash TUN cannot
/// capture this kernel's outbound connections to the proxy server.
pub(crate) fn default_interface() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
            .ok()?;
        let text = String::from_utf8(output.stdout).ok()?;
        let iface = text.lines().find_map(|line| {
            let line = line.trim();
            line.strip_prefix("interface:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })?;
        if iface.starts_with("utun") || iface.starts_with("lo") {
            return None;
        }
        Some(iface)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn top_level_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        if !is_top_level_key(line, key) {
            return None;
        }
        let raw = line.split_once(':')?.1.trim();
        Some(unquote(raw))
    })
}

fn is_top_level_key(line: &str, key: &str) -> bool {
    if line.starts_with([' ', '\t']) {
        return false;
    }
    let Some(rest) = line.strip_prefix(key) else {
        return false;
    };
    rest.trim_start().starts_with(':')
}

fn unquote(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let quote = bytes[0];
        if (quote == b'"' || quote == b'\'') && bytes[bytes.len() - 1] == quote {
            return raw[1..raw.len() - 1].to_string();
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_rewrites_only_top_level_keys() {
        let source = "\
mixed-port: 7890
external-controller: \"\"
secret: \"\"
nested:
  mixed-port: 1
  secret: keep
";
        let patched = patch_config(source, "abc123", None);
        assert!(patched.contains("mixed-port: 7891\n"));
        assert!(patched.contains("external-controller: \"127.0.0.1:9097\"\n"));
        assert!(patched.contains("secret: \"abc123\"\n"));
        assert!(patched.contains("  mixed-port: 1\n"));
        assert!(patched.contains("  secret: keep\n"));
        assert_eq!(
            top_level_value(&patched, "secret").as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn patch_appends_missing_keys() {
        let patched = patch_config("mode: rule\n", "s3cret", Some("en0"));
        assert!(patched.contains("mixed-port: 7891\n"));
        assert!(patched.contains("external-controller: \"127.0.0.1:9097\"\n"));
        assert!(patched.contains("secret: \"s3cret\"\n"));
        assert!(patched.contains("interface-name: \"en0\"\n"));
    }

    #[test]
    fn patch_disables_tun_and_fills_empty_interface() {
        let source = "\
interface-name: \"\"
tun:
  enable: true
  device: FlClash
dns:
  enable: true
";
        let patched = patch_config(source, "s", Some("en0"));
        assert!(patched.contains("interface-name: \"en0\"\n"));
        assert!(patched.contains("  enable: false\n"));
        assert!(patched.contains("dns:\n  enable: true\n"));
        let kept = patch_config("interface-name: en1\n", "s", Some("en0"));
        assert!(kept.contains("interface-name: en1\n"));
        assert!(!kept.contains("interface-name: \"en0\""));
    }
}
