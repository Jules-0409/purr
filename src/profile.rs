// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    api::Api,
    settings::{atomic_write, Preferences},
    window::LiveState,
};
use futures_util::StreamExt;
use serde_yaml::{Mapping, Value};
use std::{path::Path, sync::Mutex};

const MAX_PROFILE: usize = 2 * 1024 * 1024;
static PROFILE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const AD_RULES: [&str; 4] = [
    "DOMAIN-SUFFIX,doubleclick.net,REJECT",
    "DOMAIN-SUFFIX,googlesyndication.com,REJECT",
    "DOMAIN-SUFFIX,googleadservices.com,REJECT",
    "DOMAIN-SUFFIX,adservice.google.com,REJECT",
];
fn yaml_setting(text: &str, prefs: &Preferences, key: &str) -> Result<String, String> {
    if !matches!(key, "dns" | "block-ads") {
        return Ok(prefs.patch_config(text));
    }
    let mut root: Value = serde_yaml::from_str(text).map_err(|_| "配置不是有效的 Clash YAML")?;
    let map = root.as_mapping_mut().ok_or("配置根节点必须是映射")?;
    if key == "dns" {
        let dns = map
            .entry(Value::String("dns".into()))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        let dns = dns.as_mapping_mut().ok_or("DNS 配置必须是映射")?;
        dns.insert(Value::String("enable".into()), Value::Bool(true));
        dns.entry(Value::String("default-nameserver".into()))
            .or_insert_with(|| {
                Value::Sequence(vec![
                    Value::String("223.5.5.5".into()),
                    Value::String("119.29.29.29".into()),
                    Value::String("1.1.1.1".into()),
                ])
            });
        let endpoint = match prefs.dns.as_str() {
            "8.8.8.8" => "https://dns.google/dns-query",
            "223.5.5.5" => "https://dns.alidns.com/dns-query",
            _ => "https://cloudflare-dns.com/dns-query",
        };
        let fallback = if endpoint.contains("dns.google") {
            "https://cloudflare-dns.com/dns-query"
        } else {
            "https://dns.google/dns-query"
        };
        dns.insert(
            Value::String("nameserver".into()),
            // Every nameserver runs for every lookup, so a plain domestic resolver here
            // answers foreign names too and shows up on leak tests. Pair the pick with a
            // second foreign DoH instead; CN names keep nameserver-policy.
            Value::Sequence(vec![
                Value::String(endpoint.into()),
                Value::String(fallback.into()),
            ]),
        );
    } else {
        let rules = map
            .entry(Value::String("rules".into()))
            .or_insert_with(|| Value::Sequence(vec![]));
        let rules = rules.as_sequence_mut().ok_or("规则必须是列表")?;
        rules.retain(|r| {
            !r.as_str()
                .is_some_and(|s| AD_RULES.contains(&s) || s == "GEOSITE,category-ads-all,REJECT")
        });
        if prefs.block_ads {
            for rule in AD_RULES.into_iter().rev() {
                rules.insert(0, Value::String(rule.into()));
            }
        }
    }
    serde_yaml::to_string(&root).map_err(|_| "配置编码失败".into())
}
pub async fn update(
    api: &Api,
    live: &Mutex<LiveState>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let _serial = PROFILE_LOCK.lock().await;
    let old = live.lock().map_err(|_| "状态不可用")?.preferences.clone();
    let next = old.changed(key, value)?;
    if next == old {
        return Ok(());
    }
    if next.mixed_port != old.mixed_port {
        let host = if next.allow_lan {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };
        let _probe = std::net::TcpListener::bind((host, next.mixed_port))
            .map_err(|_| "端口已被占用，请选择另一个端口")?;
    }
    let dir = crate::config::app_config_dir()?;
    let path = dir.join("config.yaml");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let patched = yaml_setting(&text, &next, key)?;
    if key == "auto-wake" {
        crate::settings::set_autostart(next.auto_wake)?;
    }
    if patched != text {
        validate(&dir, &patched).await?;
        atomic_write(&path, patched.as_bytes())?;
        if let Err(error) = api.reload_path(&path).await {
            atomic_write(&path, text.as_bytes())?;
            let _ = api.reload_path(&path).await;
            return Err(error);
        }
        if matches!(key, "mixed-port" | "allow-lan" | "ipv6") {
            let _ = api
                .patch_configs(serde_json::json!({
                    "mixed-port": next.mixed_port,
                    "allow-lan": next.allow_lan,
                    "ipv6": next.ipv6,
                }))
                .await;
        }
    }
    if let Err(error) = next.save(&dir) {
        if patched != text {
            let _ = atomic_write(&path, text.as_bytes());
            let _ = api.reload_path(&path).await;
        }
        if key == "auto-wake" {
            let _ = crate::settings::set_autostart(old.auto_wake);
        }
        return Err(error);
    }
    crate::config::set_mixed_port(next.mixed_port);
    let awake = live.lock().map_err(|_| "状态不可用")?.awake;
    let recapture =
        awake && next.system_proxy && (next.mixed_port != old.mixed_port || !old.system_proxy);
    if recapture {
        if let Err(error) = crate::proxy::enable() {
            crate::config::set_mixed_port(old.mixed_port);
            let _ = old.save(&dir);
            if patched != text {
                let _ = atomic_write(&path, text.as_bytes());
                let _ = api.reload_path(&path).await;
            }
            return Err(error);
        }
    } else if awake && old.system_proxy && !next.system_proxy {
        crate::proxy::restore();
    }
    if key == "sound" {
        crate::sound::play(value, awake);
    }
    let mut state = live.lock().map_err(|_| "状态不可用")?;
    state.preferences = next;
    state.last_error.clear();
    if key == "mixed-port" && state.trouble == "port" {
        state.trouble.clear();
    }
    state.bump();
    drop(state);
    Ok(())
}
pub async fn import(api: &Api, live: &Mutex<LiveState>, url: &str) -> Result<(), String> {
    download(api, live, url, true, false, None).await
}
pub async fn import_file(api: &Api, live: &Mutex<LiveState>, path: &Path) -> Result<(), String> {
    let url = reqwest::Url::from_file_path(path).map_err(|_| "配置文件路径无效")?;
    download(api, live, url.as_str(), true, true, None).await
}
pub async fn update_subscription(
    api: &Api,
    live: &Mutex<LiveState>,
    id: u64,
) -> Result<(), String> {
    let url = live
        .lock()
        .map_err(|_| "状态不可用")?
        .preferences
        .subscriptions
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.url.clone())
        .ok_or("订阅不存在")?;
    download(api, live, &url, false, true, None).await
}
pub async fn edit_subscription(
    api: &Api,
    live: &Mutex<LiveState>,
    id: u64,
    url: &str,
) -> Result<(), String> {
    download(api, live, url, false, false, Some(id)).await
}
struct CacheUpdate {
    path: std::path::PathBuf,
    previous: Option<Vec<u8>>,
    committed: bool,
}
impl CacheUpdate {
    fn write(path: std::path::PathBuf, bytes: &[u8]) -> Result<Self, String> {
        let previous = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("订阅缓存读取失败".into()),
        };
        atomic_write(&path, bytes)?;
        Ok(Self {
            path,
            previous,
            committed: false,
        })
    }
}
impl Drop for CacheUpdate {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(previous) = &self.previous {
                let _ = atomic_write(&self.path, previous);
            } else {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}
fn prepare_text(api: &Api, prefs: &Preferences, text: &str) -> Result<String, String> {
    let patched = crate::config::patch_config(
        text,
        api.bearer().unwrap_or(""),
        crate::config::default_interface().as_deref(),
    );
    let patched = prefs.patch_config(&patched);
    let patched = yaml_setting(&patched, prefs, "dns")?;
    let patched = yaml_setting(&patched, prefs, "block-ads")?;
    let patched = crate::settings::set_yaml_key(&patched, "external-ui", "\"\"");
    Ok(crate::settings::set_yaml_key(
        &patched,
        "external-controller-tls",
        "\"\"",
    ))
}
async fn download(
    api: &Api,
    live: &Mutex<LiveState>,
    url: &str,
    activate: bool,
    allow_file: bool,
    replace_id: Option<u64>,
) -> Result<(), String> {
    let _serial = PROFILE_LOCK.lock().await;
    let url = url.trim();
    let parsed = reqwest::Url::parse(url).map_err(|_| "订阅地址无效")?;
    let (text, headers) = if parsed.scheme() == "file" && allow_file {
        use base64::Engine;
        use std::io::Read;
        let file = parsed.to_file_path().map_err(|_| "配置文件路径无效")?;
        if !file
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
        {
            return Err("请拖入 .yaml 或 .yml 配置文件".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&file)
            .map_err(|_| "配置文件读取失败")?
            .take(MAX_PROFILE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "配置文件读取失败")?;
        if bytes.len() > MAX_PROFILE {
            return Err("配置文件超过 2 MB".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "配置文件不是 UTF-8 YAML")?;
        let name = file
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("本地配置");
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "profile-title",
            format!(
                "base64:{}",
                base64::engine::general_purpose::STANDARD.encode(name)
            )
            .parse()
            .map_err(|_| "配置文件名称无效")?,
        );
        (text, headers)
    } else {
        if !matches!(parsed.scheme(), "https" | "http")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("订阅地址必须是 HTTP / HTTPS 链接".into());
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .danger_accept_invalid_certs(true)
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("clash.meta/Purr")
            .build()
            .map_err(|_| "无法创建订阅请求")?;
        // Try direct first; if that fails (e.g. subscription requires existing system proxy), retry with system proxy.
        let response = match client.get(parsed.clone()).send().await {
            Ok(resp) => resp,
            Err(_) => {
                let fallback_client = reqwest::Client::builder()
                    .danger_accept_invalid_certs(true)
                    .connect_timeout(std::time::Duration::from_secs(5))
                    .timeout(std::time::Duration::from_secs(15))
                    .user_agent("clash.meta/Purr")
                    .build()
                    .map_err(|_| "无法创建订阅请求")?;
                fallback_client
                    .get(parsed)
                    .send()
                    .await
                    .map_err(|_| "订阅下载失败，请检查链接与网络")?
            }
        };
        if !response.status().is_success() {
            return Err(format!("订阅服务器返回 {}", response.status()));
        }
        let headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|n| n > MAX_PROFILE as u64)
        {
            return Err("订阅配置超过 2 MB".into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "订阅读取失败")?;
            if bytes.len() + chunk.len() > MAX_PROFILE {
                return Err("订阅配置超过 2 MB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| "订阅不是 UTF-8 YAML 配置")?
            .trim_start_matches('\u{feff}')
            .to_string();
        (text, headers)
    };
    let mut prefs = live.lock().map_err(|_| "状态不可用")?.preferences.clone();
    let dir = crate::config::app_config_dir()?;
    let path = dir.join("config.yaml");
    let old = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let patched = prepare_text(api, &prefs, &text)?;
    validate(&dir, &patched).await?;
    if let Some(id) = replace_id {
        if !prefs.subscriptions.iter().any(|s| s.id == id) {
            return Err("订阅不存在".into());
        }
        if prefs
            .subscriptions
            .iter()
            .any(|s| s.id != id && s.url == url)
        {
            return Err("这个链接已属于另一个订阅".into());
        }
    }
    let replacing_active = replace_id.is_some_and(|id| {
        prefs
            .subscriptions
            .iter()
            .any(|s| s.id == id && Some(&s.url) == prefs.subscription.as_ref())
    });
    let id = replace_id.unwrap_or_else(|| {
        prefs
            .subscriptions
            .iter()
            .find(|s| s.url == url)
            .map(|s| s.id)
            .unwrap_or_else(|| prefs.subscriptions.iter().map(|s| s.id).max().unwrap_or(0) + 1)
    });
    let sub = crate::subscriptions::metadata(url, &headers, &text, id);
    let cache = dir.join("subscriptions");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    if let Some(active) = prefs
        .subscriptions
        .iter()
        .find(|s| Some(&s.url) == prefs.subscription.as_ref())
    {
        let previous = cache.join(format!("{}.yaml", active.id));
        if !previous.exists() {
            atomic_write(&previous, old.as_bytes())?;
        }
    }
    let mut cache_update = CacheUpdate::write(cache.join(format!("{id}.yaml")), text.as_bytes())?;
    let activate = activate || replacing_active || prefs.subscription.as_deref() == Some(url);
    if let Some(slot) = prefs.subscriptions.iter_mut().find(|s| s.id == id) {
        *slot = sub;
    } else {
        prefs.subscriptions.push(sub);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    if activate {
        atomic_write(
            &dir.join(format!("config.before-import-{stamp}.yaml")),
            old.as_bytes(),
        )?;
        atomic_write(&path, patched.as_bytes())?;
        if let Err(e) = api.reload_path(&path).await {
            atomic_write(&path, old.as_bytes())?;
            let _ = api.reload_path(&path).await;
            return Err(e);
        }
        crate::status::release_unused_memory();
        prefs.subscription = Some(url.into());
    }
    if let Err(e) = prefs.save(&dir) {
        if activate {
            atomic_write(&path, old.as_bytes())?;
            let _ = api.reload_path(&path).await;
        }
        return Err(e);
    }
    cache_update.committed = true;
    let mut state = live.lock().map_err(|_| "状态不可用")?;
    state.preferences = prefs;
    state.last_error.clear();
    if state.trouble == "sub" {
        state.trouble.clear();
    }
    state.bump();
    Ok(())
}
pub async fn use_subscription(api: &Api, live: &Mutex<LiveState>, id: u64) -> Result<(), String> {
    let _serial = PROFILE_LOCK.lock().await;
    let mut prefs = live.lock().map_err(|_| "状态不可用")?.preferences.clone();
    let sub = prefs
        .subscriptions
        .iter()
        .find(|s| s.id == id)
        .ok_or("订阅不存在")?;
    if sub.exhausted() {
        return Err("这个订阅已到期或额度用尽，请先更新".into());
    }
    let dir = crate::config::app_config_dir()?;
    let source = std::fs::read_to_string(dir.join("subscriptions").join(format!("{id}.yaml")))
        .map_err(|_| "没有可用缓存，请先更新这个订阅")?;
    let patched = prepare_text(api, &prefs, &source)?;
    validate(&dir, &patched).await?;
    let path = dir.join("config.yaml");
    let old = std::fs::read(&path).map_err(|e| e.to_string())?;
    atomic_write(&path, patched.as_bytes())?;
    if let Err(e) = api.reload_path(&path).await {
        atomic_write(&path, &old)?;
        let _ = api.reload_path(&path).await;
        return Err(e);
    }
    crate::status::release_unused_memory();
    prefs.subscription = Some(sub.url.clone());
    if let Err(e) = prefs.save(&dir) {
        atomic_write(&path, &old)?;
        let _ = api.reload_path(&path).await;
        return Err(e);
    }
    let mut state = live.lock().map_err(|_| "状态不可用")?;
    state.preferences = prefs;
    state.trouble.clear();
    state.last_error.clear();
    state.bump();
    Ok(())
}
async fn validate(dir: &Path, text: &str) -> Result<(), String> {
    let _ = crate::config::copy_geodata(dir);
    let root: Value =
        serde_yaml::from_str(text).map_err(|_| "配置校验失败，原配置已保留".to_string())?;
    let map = root
        .as_mapping()
        .ok_or_else(|| "配置校验失败，原配置已保留".to_string())?;
    if !map.contains_key(Value::String("proxies".into()))
        && !map.contains_key(Value::String("proxy-providers".into()))
    {
        return Err("订阅中没有找到节点 (proxies)，请确认是 Clash 订阅链接".into());
    }
    if dir.join("purr-validation-fixture").exists() {
        let candidate = dir.join("config.purr-validate.yaml");
        atomic_write(&candidate, text.as_bytes())?;
        let mut command = tokio::process::Command::new(crate::config::core_binary());
        command
            .arg("-t")
            .arg("-d")
            .arg(dir)
            .arg("-f")
            .arg(&candidate)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(15), command.status()).await;
        let _ = std::fs::remove_file(&candidate);
        return match result {
            Ok(Ok(status)) if status.success() => Ok(()),
            _ => Err("配置校验失败，原配置已保留".into()),
        };
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dns_edit_preserves_other_dns_keys() {
        let p = Preferences::default();
        let source="dns:\n  enable: false\n  enhanced-mode: fake-ip\n  nameserver: [8.8.8.8]\nproxies: []\n";
        let changed = yaml_setting(source, &p, "dns").unwrap();
        let v: Value = serde_yaml::from_str(&changed).unwrap();
        assert_eq!(v["dns"]["enhanced-mode"].as_str(), Some("fake-ip"));
        assert_eq!(
            v["dns"]["nameserver"][0].as_str(),
            Some("https://cloudflare-dns.com/dns-query")
        );
    }
    #[test]
    fn ad_rules_precede_catch_all_and_can_be_removed() {
        let mut p = Preferences::default();
        let text = yaml_setting("rules: ['MATCH,DIRECT']\n", &p, "block-ads").unwrap();
        let v: Value = serde_yaml::from_str(&text).unwrap();
        assert_eq!(v["rules"][0].as_str(), Some(AD_RULES[0]));
        assert_eq!(
            v["rules"].as_sequence().unwrap().last().unwrap().as_str(),
            Some("MATCH,DIRECT")
        );
        p.block_ads = false;
        let v: Value =
            serde_yaml::from_str(&yaml_setting(&text, &p, "block-ads").unwrap()).unwrap();
        assert!(!v["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|r| r.as_str() == Some(AD_RULES[0])));
    }
}
