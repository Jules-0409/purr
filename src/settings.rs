// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub auto_wake: bool,
    pub auto_swap: bool,
    pub block_ads: bool,
    pub close_to_tray: bool,
    pub system_proxy: bool,
    /// Whether the user wants TUN on. Kept so a core restart or reload can bring it back.
    pub tun: bool,
    pub mixed_port: u16,
    pub allow_lan: bool,
    pub ipv6: bool,
    pub dns: String,
    pub sound: String,
    pub subscription: Option<String>,
    pub subscriptions: Vec<Subscription>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Subscription {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub upload: Option<u64>,
    pub download: Option<u64>,
    pub total: Option<u64>,
    pub expires: Option<u64>,
    pub updated: u64,
    pub nodes: usize,
    pub homepage: Option<String>,
}
impl Subscription {
    pub fn exhausted(&self) -> bool {
        self.expires
            .is_some_and(|t| t > 0 && t <= crate::subscriptions::now())
            || self
                .total
                .is_some_and(|t| t > 0 && self.used().unwrap_or(0) >= t)
    }
    pub fn used(&self) -> Option<u64> {
        self.upload
            .zip(self.download)
            .map(|(u, d)| u.saturating_add(d))
    }
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            auto_wake: false,
            auto_swap: true,
            block_ads: true,
            close_to_tray: true,
            system_proxy: true,
            tun: false,
            mixed_port: crate::config::MIXED_PORT,
            allow_lan: false,
            ipv6: true,
            dns: "1.1.1.1".into(),
            sound: "off".into(),
            subscription: None,
            subscriptions: Vec::new(),
        }
    }
}
impl Preferences {
    pub fn load(dir: &Path) -> Result<Self, String> {
        match fs::read(dir.join("preferences.json")) {
            Ok(bytes) => {
                let mut prefs: Self =
                    serde_json::from_slice(&bytes).map_err(|e| format!("读取偏好设置失败：{e}"))?;
                if prefs.subscriptions.is_empty() {
                    if let Some(url) = prefs.subscription.clone() {
                        prefs.subscriptions.push(Subscription {
                            id: 1,
                            name: crate::subscriptions::url_name(&url),
                            url,
                            ..Subscription::default()
                        });
                    }
                }
                Ok(prefs)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        atomic_write(&dir.join("preferences.json"), &bytes)
    }
    pub fn changed(&self, key: &str, value: &str) -> Result<Self, String> {
        let mut next = self.clone();
        let boolean = || value.parse::<bool>().map_err(|_| "开关值无效".to_string());
        match key {
            "auto-wake" => next.auto_wake = boolean()?,
            "auto-swap" => next.auto_swap = boolean()?,
            "block-ads" => next.block_ads = boolean()?,
            "close-to-tray" => next.close_to_tray = boolean()?,
            "system-proxy" => next.system_proxy = boolean()?,
            "allow-lan" => next.allow_lan = boolean()?,
            "ipv6" => next.ipv6 = boolean()?,
            "mixed-port" => {
                next.mixed_port = value
                    .parse::<u16>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or("端口必须为 1–65535")?
            }
            "dns" if ["1.1.1.1", "8.8.8.8", "223.5.5.5"].contains(&value) => {
                next.dns = value.into()
            }
            "sound" if ["off", "ping", "loop"].contains(&value) => next.sound = value.into(),
            _ => return Err("未知设置".into()),
        }
        Ok(next)
    }
    pub fn patch_config(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for (key, value) in [
            ("mixed-port", self.mixed_port.to_string()),
            ("allow-lan", self.allow_lan.to_string()),
            ("ipv6", self.ipv6.to_string()),
        ] {
            text = set_yaml_key(&text, key, &value);
        }
        drop_plain_nameservers(&text)
    }
}

/// `dns.nameserver` entries all race for every lookup, so a plain UDP resolver listed
/// beside a DoH one answers foreign names too and shows up on DNS leak tests. Keep the
/// encrypted ones (CN names still go through nameserver-policy). Lists with no DoH
/// entry are left alone so resolution never ends up empty.
pub fn drop_plain_nameservers(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < lines.len() {
        out.push_str(lines[i]);
        if lines[i].trim_end() == "  nameserver:" {
            let mut j = i + 1;
            while j < lines.len() && lines[j].starts_with("  - ") {
                j += 1;
            }
            let items = &lines[i + 1..j];
            let encrypted = |l: &str| {
                let v = l.trim_start_matches("  - ").trim().trim_matches(|c| c == '\'' || c == '"');
                v.contains("://")
            };
            let has_doh = items.iter().any(|l| encrypted(l));
            for l in items {
                if !has_doh || encrypted(l) {
                    out.push_str(l);
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod nameserver_tests {
    use super::drop_plain_nameservers;
    #[test]
    fn drops_plain_beside_doh() {
        let t = "dns:\n  nameserver:\n  - https://cloudflare-dns.com/dns-query\n  - 223.5.5.5\n  nameserver-policy:\n    geosite:cn:\n    - 223.5.5.5\n";
        let o = drop_plain_nameservers(t);
        assert!(o.contains("  - https://cloudflare-dns.com/dns-query\n  nameserver-policy:"));
        assert!(o.contains("    - 223.5.5.5\n"));
    }
    #[test]
    fn keeps_plain_only_list() {
        let t = "dns:\n  nameserver:\n  - 223.5.5.5\n";
        assert_eq!(drop_plain_nameservers(t), t);
    }
}
/// Version of the first-launch notice; bump it to ask everyone to read a changed text again.
pub const DISCLAIMER_VERSION: u32 = 1;
/// What the user has to type before the client starts anything.
pub const DISCLAIMER_PHRASE: &str = "我已阅读并同意";
const DISCLAIMER_FILE: &str = "disclaimer.json";

/// Stored next to preferences.json in the app config directory. Kept in its own file so
/// preference writes (which serialize in-memory copies) can never drop the acceptance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DisclaimerAcceptance {
    pub disclaimer_accepted: bool,
    pub version: u32,
    /// Unix seconds.
    pub accepted_at: u64,
}

pub fn disclaimer_input_matches(input: &str) -> bool {
    input.trim() == DISCLAIMER_PHRASE
}

pub fn disclaimer_accepted(dir: &Path) -> bool {
    fs::read(dir.join(DISCLAIMER_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<DisclaimerAcceptance>(&bytes).ok())
        .is_some_and(|a| a.disclaimer_accepted && a.version >= DISCLAIMER_VERSION)
}

pub fn accept_disclaimer(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let record = DisclaimerAcceptance {
        disclaimer_accepted: true,
        version: DISCLAIMER_VERSION,
        accepted_at: crate::subscriptions::now(),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
    atomic_write(&dir.join(DISCLAIMER_FILE), &bytes)
}

/// Replace only a complete top-level scalar, preserving unrelated profile content.
pub fn set_yaml_key(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out = String::new();
    for line in text.lines() {
        if !line.starts_with([' ', '\t', '#'])
            && line.split_once(':').is_some_and(|(k, _)| k.trim() == key)
        {
            out.push_str(&format!("{key}: {value}\n"));
            found = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        out.push_str(&format!("{key}: {value}\n"));
    }
    out
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("purr-tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temp).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    // ReplaceFileW semantics are needed when a target already exists on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        // A LaunchAgent needs no eagerly linked ServiceManagement/SwiftUI stack.
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let dir = std::path::PathBuf::from(home).join("Library/LaunchAgents");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("com.purr.desktop.plist");
        if enabled {
            let exe = exe
                .to_str()
                .ok_or("应用路径无效")?
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;");
            atomic_write(&path,format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.purr.desktop</string><key>ProgramArguments</key><array><string>{exe}</string><string>--minimized</string></array><key>RunAtLoad</key><true/></dict></plist>\n").as_bytes())
        } else {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
    #[cfg(windows)]
    {
        let mut builder = auto_launch::AutoLaunchBuilder::new();
        builder
            .set_app_name("Purr")
            .set_app_path(exe.to_str().ok_or("应用路径无效")?)
            .set_args(&["--minimized"]);
        #[cfg(windows)]
        builder.set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
        let auto = builder.build().map_err(|e| e.to_string())?;
        if enabled {
            auto.enable()
        } else {
            auto.disable()
        }
        .map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (exe, enabled);
        Err("此平台尚不支持开机启动".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setting_validation_and_nested_profile_preservation() {
        let p = Preferences::default();
        for invalid in ["0", "65536", "-1", "7890\nsecret: x"] {
            assert!(p.changed("mixed-port", invalid).is_err());
        }
        let text = "mixed-port: 7\nnested:\n  mixed-port: 8\nproxies: []\n";
        let changed = p.changed("mixed-port", "9090").unwrap().patch_config(text);
        assert!(changed.contains("mixed-port: 9090\n"));
        assert!(changed.contains("  mixed-port: 8\n"));
        assert!(changed.contains("proxies: []\n"));
        assert_eq!(set_yaml_key(&changed, "mixed-port", "9090"), changed);
    }
    #[test]
    fn disclaimer_phrase_and_flag() {
        assert!(disclaimer_input_matches("我已阅读并同意"));
        assert!(disclaimer_input_matches("  我已阅读并同意\n"));
        assert!(disclaimer_input_matches("\u{3000}我已阅读并同意\u{3000}"));
        for wrong in [
            "",
            "我已阅读",
            "我已阅读 并同意",
            "我已阅读并同意。",
            "同意",
        ] {
            assert!(!disclaimer_input_matches(wrong), "{wrong:?}");
        }
        let dir = std::env::temp_dir().join(format!("purr-disclaimer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert!(!disclaimer_accepted(&dir));
        accept_disclaimer(&dir).unwrap();
        assert!(disclaimer_accepted(&dir));
        let saved: DisclaimerAcceptance =
            serde_json::from_slice(&fs::read(dir.join(DISCLAIMER_FILE)).unwrap()).unwrap();
        assert!(saved.disclaimer_accepted);
        assert_eq!(saved.version, DISCLAIMER_VERSION);
        assert!(saved.accepted_at > 0);
        // A preference write must not touch the acceptance.
        Preferences::default().save(&dir).unwrap();
        assert!(disclaimer_accepted(&dir));
        // Older or broken records ask again.
        fs::write(
            dir.join(DISCLAIMER_FILE),
            br#"{"disclaimer_accepted":true,"version":0,"accepted_at":1}"#,
        )
        .unwrap();
        assert!(!disclaimer_accepted(&dir));
        fs::write(dir.join(DISCLAIMER_FILE), b"not json").unwrap();
        assert!(!disclaimer_accepted(&dir));
        fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn preferences_round_trip() {
        let path = std::env::temp_dir().join(format!("purr-prefs-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        let p = Preferences::default().changed("allow-lan", "true").unwrap();
        p.save(&path).unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), p);
        fs::remove_file(path.join("preferences.json")).unwrap();
        fs::remove_dir(path).unwrap();
    }
}
