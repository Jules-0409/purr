// SPDX-License-Identifier: GPL-3.0-only

//! C-ABI FFI bindings for macOS SwiftUI native frontend to drive Purr's real Rust + mihomo engine.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use futures_util::{stream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use serde_yaml::Mapping;

use crate::api::Api;
use crate::core::Core;
use crate::settings::{atomic_write, Preferences};

static IS_AWAKE: AtomicBool = AtomicBool::new(false);
static NEED_PLACE_REFRESH: AtomicBool = AtomicBool::new(true);

struct RuntimeCtx {
    handle: tokio::runtime::Handle,
    api: Api,
    state: Arc<Mutex<FfiState>>,
    _core: Mutex<Option<Core>>,
}

static CTX: OnceLock<Arc<RuntimeCtx>> = OnceLock::new();

#[derive(Clone, Debug, Default, Serialize)]
pub struct FfiNode {
    pub name: String,
    pub proto: String,
    pub ms: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FfiGroup {
    pub name: String,
    pub kind: String,
    pub now: String,
    pub nodes: Vec<FfiNode>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FfiSub {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub desc: String,
    /// Whole GB from the subscription-userinfo header; null when the provider does not send it.
    pub used: Option<u64>,
    pub total: Option<u64>,
    pub exp: String,
    pub days_left: Option<i64>,
    pub active: bool,
    pub exhausted: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FfiConn {
    pub id: String,
    pub app: String,
    pub host: String,
    pub rule: String,
    pub pol: String,
    pub chain: String,
    pub dn_kb: f64,
    pub up_kb: f64,
    pub age_s: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FfiRule {
    pub kind: String,
    pub match_str: String,
    pub policy: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FfiSnapshot {
    pub ready: bool,
    pub awake: bool,
    pub first_run: bool,
    pub mode: String,
    pub tun: bool,
    pub dn_mb_s: f64,
    pub up_mb_s: f64,
    pub proxy_mb: f64,
    pub direct_mb: f64,
    pub blocked_count: u64,
    pub mem_mb: f64,
    pub exit_ip: String,
    pub exit_loc: String,
    pub direct_ip: String,
    pub direct_loc: String,
    pub groups: Vec<FfiGroup>,
    pub subscriptions: Vec<FfiSub>,
    pub connections: Vec<FfiConn>,
    pub rules: Vec<FfiRule>,
}

struct FfiState {
    ready: bool,
    mode: String,
    tun: bool,
    dn_mb_s: f64,
    up_mb_s: f64,
    last_dn_total: Option<u64>,
    last_up_total: Option<u64>,
    last_rate_at: Option<Instant>,
    proxy_bytes: u64,
    direct_bytes: u64,
    seen_conn_bytes: HashMap<String, (u64, u64)>,
    blocked_count: u64,
    mem_mb: f64,
    exit_ip: String,
    exit_loc: String,
    direct_ip: String,
    direct_loc: String,
    groups: Vec<FfiGroup>,
    /// 显示名（去掉 emoji）→ mihomo 里的真实名字，切换节点时要用真实名字
    raw_names: HashMap<String, String>,
    delays: HashMap<String, u32>,
    connections: Vec<FfiConn>,
    rules: Vec<FfiRule>,
    preferences: Preferences,
}

impl Default for FfiState {
    fn default() -> Self {
        Self {
            ready: false,
            mode: "rule".into(),
            tun: false,
            dn_mb_s: 0.0,
            up_mb_s: 0.0,
            last_dn_total: None,
            last_up_total: None,
            last_rate_at: None,
            proxy_bytes: 0,
            direct_bytes: 0,
            seen_conn_bytes: HashMap::new(),
            blocked_count: 0,
            mem_mb: 0.0, // 0 = not measured yet; the UI shows "—"
            exit_ip: "—".into(),
            exit_loc: "直连未经过窝点".into(),
            direct_ip: "检测中…".into(),
            direct_loc: "本地宽带直连出口".into(),
            groups: Vec::new(),
            raw_names: HashMap::new(),
            delays: HashMap::new(),
            connections: Vec::new(),
            rules: Vec::new(),
            preferences: Preferences::default(),
        }
    }
}

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
    let mut root: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|_| "配置不是有效的 Clash YAML")?;
    let map = root.as_mapping_mut().ok_or("配置根节点必须是映射")?;
    if key == "dns" {
        let dns = map
            .entry(serde_yaml::Value::String("dns".into()))
            .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
        let dns = dns.as_mapping_mut().ok_or("DNS 配置必须是映射")?;
        dns.insert(
            serde_yaml::Value::String("enable".into()),
            serde_yaml::Value::Bool(true),
        );
        let endpoint = match prefs.dns.as_str() {
            "8.8.8.8" => "https://dns.google/dns-query",
            "223.5.5.5" => "https://dns.alidns.com/dns-query",
            _ => "https://cloudflare-dns.com/dns-query",
        };
        dns.insert(
            serde_yaml::Value::String("nameserver".into()),
            serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::String(endpoint.into()),
                serde_yaml::Value::String("223.5.5.5".into()),
            ]),
        );
    } else {
        let rules = map
            .entry(serde_yaml::Value::String("rules".into()))
            .or_insert_with(|| serde_yaml::Value::Sequence(vec![]));
        let rules = rules.as_sequence_mut().ok_or("规则必须是列表")?;
        rules.retain(|r| {
            !r.as_str()
                .is_some_and(|s| AD_RULES.contains(&s) || s == "GEOSITE,category-ads-all,REJECT")
        });
        if prefs.block_ads {
            for rule in AD_RULES.into_iter().rev() {
                rules.insert(0, serde_yaml::Value::String(rule.into()));
            }
        }
    }
    serde_yaml::to_string(&root).map_err(|_| "配置编码失败".into())
}

fn prepare_yaml_text(api: &Api, prefs: &Preferences, text: &str) -> Result<String, String> {
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

#[no_mangle]
pub extern "C" fn purr_core_init() -> c_int {
    if CTX.get().is_some() {
        return 0;
    }
    crate::proxy::install_panic_hook();
    crate::proxy::restore();

    let settings = match crate::config::prepare() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("purr_core_init: prepare failed: {e}");
            return -1;
        }
    };

    let prefs = Preferences::load(&settings.config_dir).unwrap_or_default();
    let initial_awake = prefs.auto_wake;
    IS_AWAKE.store(initial_awake, Ordering::Relaxed);

    let state = Arc::new(Mutex::new(FfiState {
        preferences: prefs.clone(),
        ..FfiState::default()
    }));

    let api = Api::new(settings.secret.clone());
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let state_bg = Arc::clone(&state);
    let api_bg = api.clone();
    let bin_bg = settings.core_binary.clone();
    let dir_bg = settings.config_dir.clone();

    std::thread::Builder::new()
        .name("purr-core-rt".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("purr tokio runtime");
            let handle = rt.handle().clone();
            rt.block_on(async move {
                let core = Core::start(bin_bg, dir_bg);
                let _ = tx.send((handle, core));
                background_loop(api_bg, state_bg, initial_awake).await;
            });
        })
        .ok();

    if let Ok((handle, core)) = rx.recv_timeout(Duration::from_secs(5)) {
        let _ = CTX.set(Arc::new(RuntimeCtx {
            handle,
            api,
            state,
            _core: Mutex::new(Some(core)),
        }));
        0
    } else {
        -2
    }
}

async fn background_loop(api: Api, state: Arc<Mutex<FfiState>>, initial_awake: bool) {
    let _ = api.wait_ready().await;
    if initial_awake {
        let sys = state.lock().map(|s| s.preferences.system_proxy).unwrap_or(true);
        if sys {
            let _ = crate::proxy::enable();
        }
    }
    tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            if let Some((ip, loc)) = lookup_direct_place().await {
                if let Ok(mut g) = state.lock() {
                    g.direct_ip = ip;
                    g.direct_loc = loc;
                }
            }
        }
    });

    let mut last_place_check = Instant::now() - Duration::from_secs(60);
    let mut tick_count = 0u64;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        tick_count = tick_count.wrapping_add(1);
        refresh_from_mihomo(&api, &state, tick_count).await;

        let awake = IS_AWAKE.load(Ordering::Relaxed);
        let need_place = NEED_PLACE_REFRESH.swap(false, Ordering::Relaxed);
        if awake && (need_place || last_place_check.elapsed() >= Duration::from_secs(20)) {
            last_place_check = Instant::now();
            let state_place = Arc::clone(&state);
            tokio::spawn(async move {
                let place = crate::status::lookup_place().await;
                if place != "暂时不知道" {
                    let (ip, loc) = match place.split_once(" · ") {
                        Some((i, l)) => (i.trim().to_string(), l.trim().to_string()),
                        None => (place, "代理出口".to_string()),
                    };
                    if let Ok(mut g) = state_place.lock() {
                        g.exit_ip = ip;
                        g.exit_loc = loc;
                    }
                }
            });
        }
    }
}

async fn lookup_direct_place() -> Option<(String, String)> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(4))
        .user_agent("purr")
        .build()
        .ok()?;
    for url in [
        "https://ipwho.is/",
        "http://ip-api.com/json?fields=status,query,city,countryCode",
        "https://ipinfo.io/json",
    ] {
        let Ok(resp) = client.get(url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(val) = resp.json::<Value>().await else {
            continue;
        };
        if let Some(place) = crate::present::place_from_json(&val) {
            return match place.split_once(" · ") {
                Some((i, l)) => Some((i.trim().to_string(), format!("{l} · 本地宽带直连出口"))),
                None => Some((place, "本地宽带直连出口".to_string())),
            };
        }
    }
    None
}

async fn refresh_from_mihomo(api: &Api, state: &Arc<Mutex<FfiState>>, tick_count: u64) {
    let shell_bytes = crate::status::footprint_bytes(std::process::id()).unwrap_or(0);
    let core_bytes = crate::status::footprint_bytes(crate::core::running_pid()).unwrap_or(0);
    let total_mem_mb = ((shell_bytes + core_bytes) as f64) / (1024.0 * 1024.0);

    let mode = match api.mode().await {
        Ok(m) => m,
        Err(_) => {
            if let Ok(mut g) = state.lock() {
                g.ready = false;
                if total_mem_mb > 1.0 {
                    g.mem_mb = total_mem_mb;
                }
            }
            return;
        }
    };

    let proxies_json = api.proxies().await.ok();
    let conns_json = api.connections().await.ok();
    let need_rules = state.lock().map(|g| g.rules.is_empty()).unwrap_or(true) || tick_count % 10 == 1;
    let rules_json = if need_rules {
        api.rules().await.ok()
    } else {
        None
    };

    let now_sys = SystemTime::now();
    let now_inst = Instant::now();

    let Ok(mut g) = state.lock() else {
        return;
    };
    g.ready = true;
    g.mode = mode;
    if total_mem_mb > 1.0 {
        g.mem_mb = total_mem_mb;
    }

    if let Some(ref pjson) = proxies_json {
        let mut parsed = crate::present::proxy_groups(pjson);
        // 手选的 select 组排最前（托盘/概览默认操作第一组），fallback/url-test 等自动组靠后，GLOBAL 仍在最后
        parsed.sort_by_key(|pg| {
            if pg.name == "GLOBAL" { 2 } else if pg.kind == "select" { 0 } else { 1 }
        });
        let mut next_groups = Vec::new();
        for pg in parsed {
            g.raw_names.insert(pg.label.clone(), pg.name.clone());
            for n in &pg.nodes {
                g.raw_names.insert(n.label.clone(), n.name.clone());
            }
            if pg.name == "GLOBAL" && g.mode != "global" {
                continue;
            }
            if pg.nodes.is_empty() {
                continue;
            }
            let nodes: Vec<FfiNode> = pg
                .nodes
                .iter()
                .map(|n| {
                    let cached_ms = g.delays.get(&n.name).copied().or(n.delay_ms).unwrap_or(0); // 0 = 未测出，界面显示「—」
                    FfiNode {
                        name: n.label.clone(),
                        proto: if n.kind.is_empty() {
                            "Proxy".into()
                        } else {
                            n.kind.clone()
                        },
                        ms: cached_ms,
                    }
                })
                .collect();
            let now_clean = crate::present::display_name(&pg.now);
            next_groups.push(FfiGroup {
                name: pg.label,
                kind: pg.kind,
                now: now_clean,
                nodes,
            });
        }
        if !next_groups.is_empty() {
            g.groups = next_groups;
        }
    }

    if let Some(ref cjson) = conns_json {
        let dn_tot = cjson
            .get("downloadTotal")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let up_tot = cjson
            .get("uploadTotal")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if let (Some(prev_dn), Some(prev_up), Some(prev_at)) =
            (g.last_dn_total, g.last_up_total, g.last_rate_at)
        {
            let dt = now_inst.duration_since(prev_at).as_secs_f64().max(0.2);
            let d_dn = dn_tot.saturating_sub(prev_dn) as f64 / (1024.0 * 1024.0) / dt;
            let d_up = up_tot.saturating_sub(prev_up) as f64 / (1024.0 * 1024.0) / dt;
            g.dn_mb_s = d_dn;
            g.up_mb_s = d_up;
        }
        g.last_dn_total = Some(dn_tot);
        g.last_up_total = Some(up_tot);
        g.last_rate_at = Some(now_inst);

        let mut active_ids = std::collections::HashSet::new();
        let mut next_conns = Vec::new();
        if let Some(arr) = cjson.get("connections").and_then(Value::as_array) {
            for item in arr {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                active_ids.insert(id.clone());
                let down = item.get("download").and_then(Value::as_u64).unwrap_or(0);
                let up = item.get("upload").and_then(Value::as_u64).unwrap_or(0);
                let chains: Vec<String> = item
                    .get("chains")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(crate::present::display_name)
                            .collect()
                    })
                    .unwrap_or_default();
                let first_chain = chains.first().cloned().unwrap_or_else(|| "DIRECT".into());
                let last_chain = chains.last().cloned().unwrap_or_else(|| "DIRECT".into());
                let is_direct = first_chain.eq_ignore_ascii_case("DIRECT")
                    || last_chain.eq_ignore_ascii_case("DIRECT");
                let is_reject = first_chain.eq_ignore_ascii_case("REJECT")
                    || last_chain.eq_ignore_ascii_case("REJECT");

                let prev = g.seen_conn_bytes.insert(id.clone(), (down, up));
                let delta = match prev {
                    Some((pd, pu)) => down.saturating_sub(pd) + up.saturating_sub(pu),
                    None => {
                        if is_reject {
                            g.blocked_count = g.blocked_count.saturating_add(1);
                        }
                        down + up
                    }
                };
                if is_direct {
                    g.direct_bytes = g.direct_bytes.saturating_add(delta);
                } else if !is_reject {
                    g.proxy_bytes = g.proxy_bytes.saturating_add(delta);
                }

                let meta = item.get("metadata");
                let host = meta
                    .and_then(|m| m.get("host"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        meta.and_then(|m| m.get("destinationIP"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("—")
                    .to_string();
                let app = meta
                    .and_then(|m| m.get("process"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        meta.and_then(|m| m.get("processPath"))
                            .and_then(Value::as_str)
                            .and_then(|p| Path::new(p).file_name()?.to_str())
                    })
                    .unwrap_or("—")
                    .to_string();
                let rule_type = item.get("rule").and_then(Value::as_str).unwrap_or("MATCH");
                let rule_payload = item
                    .get("rulePayload")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let rule = if rule_payload.is_empty() {
                    rule_type.to_string()
                } else {
                    format!("{rule_type},{rule_payload}")
                };
                let pol = if is_direct {
                    "DIRECT".to_string()
                } else if is_reject {
                    "REJECT".to_string()
                } else {
                    last_chain.clone()
                };
                let chain_display = if is_direct || is_reject {
                    String::new()
                } else if chains.len() >= 2 {
                    format!("{last_chain} → {}", chains[0])
                } else {
                    first_chain
                };
                let start_str = item.get("start").and_then(Value::as_str).unwrap_or("");
                let age_s = parse_conn_age_secs(start_str, now_sys);
                next_conns.push(FfiConn {
                    id,
                    app,
                    host,
                    rule,
                    pol,
                    chain: chain_display,
                    dn_kb: (down as f64) / 1024.0,
                    up_kb: (up as f64) / 1024.0,
                    age_s,
                });
            }
        }
        g.seen_conn_bytes.retain(|k, _| active_ids.contains(k));
        next_conns.sort_by(|a, b| {
            b.dn_kb
                .partial_cmp(&a.dn_kb)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        g.connections = next_conns;
    }

    if let Some(ref rjson) = rules_json {
        if let Some(arr) = rjson.get("rules").and_then(Value::as_array) {
            g.rules = arr
                .iter()
                .take(300)
                .map(|r| FfiRule {
                    kind: r
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("MATCH")
                        .to_uppercase(),
                    match_str: r
                        .get("payload")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("*")
                        .to_string(),
                    policy: crate::present::display_name(
                        r.get("proxy").and_then(Value::as_str).unwrap_or("DIRECT"),
                    ),
                })
                .collect();
        }
    }
}

fn parse_conn_age_secs(start: &str, now: SystemTime) -> u64 {
    if start.len() < 19 {
        return 1;
    }
    let year: i32 = start[0..4].parse().ok().unwrap_or(1970);
    let month: u32 = start[5..7].parse().ok().unwrap_or(1);
    let day: u32 = start[8..10].parse().ok().unwrap_or(1);
    let hour: u32 = start[11..13].parse().ok().unwrap_or(0);
    let minute: u32 = start[14..16].parse().ok().unwrap_or(0);
    let second: u32 = start[17..19].parse().ok().unwrap_or(0);
    let mut y = year;
    if month <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (month as i32 + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    let days = i64::from(era) * 146097 + doe as i64 - 719468;
    let unix = days * 86400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    if unix <= 0 {
        return 1;
    }
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now_secs.saturating_sub(unix as u64).max(1)
}

#[no_mangle]
pub extern "C" fn purr_core_is_awake() -> bool {
    IS_AWAKE.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn purr_core_set_awake(awake: bool) {
    let prev = IS_AWAKE.swap(awake, Ordering::Relaxed);
    if prev == awake {
        return;
    }
    let Some(ctx) = CTX.get().cloned() else {
        return;
    };
    let handle = ctx.handle.clone();
    handle.spawn(async move {
        let sys = ctx
            .state
            .lock()
            .map(|s| s.preferences.system_proxy)
            .unwrap_or(true);
        let tun = ctx.state.lock().map(|s| s.tun).unwrap_or(false);
        if awake {
            if sys {
                let _ = crate::proxy::enable();
            }
            NEED_PLACE_REFRESH.store(true, Ordering::Relaxed);
        } else {
            crate::proxy::restore();
            if tun {
                let _ = ctx
                    .api
                    .patch_configs(serde_json::json!({"tun": {"enable": false}}))
                    .await;
                if let Ok(mut g) = ctx.state.lock() {
                    g.tun = false;
                }
            }
            let _ = ctx.api.close_connections().await;
            if let Ok(mut g) = ctx.state.lock() {
                g.dn_mb_s = 0.0;
                g.up_mb_s = 0.0;
                g.connections.clear();
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn purr_core_set_mode(mode: *const c_char) {
    if mode.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(mode) }.to_string_lossy().into_owned();
    let normalized = match s.as_str() {
        "global" => "global",
        "direct" => "direct",
        _ => "rule",
    };
    let Some(ctx) = CTX.get().cloned() else {
        return;
    };
    if let Ok(mut g) = ctx.state.lock() {
        g.mode = normalized.to_string();
    }
    let handle = ctx.handle.clone();
    handle.spawn(async move {
        let _ = ctx.api.set_mode(normalized).await;
        NEED_PLACE_REFRESH.store(true, Ordering::Relaxed);
    });
}

#[no_mangle]
pub extern "C" fn purr_core_set_tun(enabled: bool) -> c_int {
    let Some(ctx) = CTX.get().cloned() else {
        return -1;
    };
    let payload = if enabled {
        serde_json::json!({
            "tun": {
                "enable": true,
                "stack": "system",
                "auto-route": true,
                "auto-detect-interface": true,
                "dns-hijack": ["any:53"]
            }
        })
    } else {
        serde_json::json!({"tun": {"enable": false}})
    };
    let res = ctx.handle.block_on(async {
        tokio::time::timeout(Duration::from_secs(4), ctx.api.patch_configs(payload)).await
    });
    match res {
        Ok(Ok(())) => {
            if let Ok(mut g) = ctx.state.lock() {
                g.tun = enabled;
            }
            0
        }
        _ => -1,
    }
}

#[no_mangle]
pub extern "C" fn purr_core_select_node(group: *const c_char, node: *const c_char) -> c_int {
    if group.is_null() || node.is_null() {
        return -1;
    }
    let g_name = unsafe { CStr::from_ptr(group) }
        .to_string_lossy()
        .into_owned();
    let n_name = unsafe { CStr::from_ptr(node) }
        .to_string_lossy()
        .into_owned();
    let Some(ctx) = CTX.get().cloned() else {
        return -1;
    };
    let (mut raw_g, mut raw_n) = (g_name.clone(), n_name.clone());
    if let Ok(mut g) = ctx.state.lock() {
        if let Some(r) = g.raw_names.get(&g_name) {
            raw_g = r.clone();
        }
        if let Some(r) = g.raw_names.get(&n_name) {
            raw_n = r.clone();
        }
        if let Some(grp) = g.groups.iter_mut().find(|item| item.name == g_name) {
            grp.now = n_name.clone();
        }
    }
    let handle = ctx.handle.clone();
    handle.spawn(async move {
        let _ = ctx.api.select(&raw_g, &raw_n).await;
        NEED_PLACE_REFRESH.store(true, Ordering::Relaxed);
    });
    0
}

#[no_mangle]
pub extern "C" fn purr_core_test_group(group: *const c_char) -> *mut c_char {
    let g_filter = if group.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(group) }
            .to_string_lossy()
            .into_owned()
    };
    let Some(ctx) = CTX.get().cloned() else {
        return CString::new("{\"ok\":false}").unwrap_or_default().into_raw();
    };
    // (显示名, 真实名)：测速要用 mihomo 里的真实名字，结果仍按显示名回给界面
    let nodes: Vec<(String, String)> = {
        let Ok(g) = ctx.state.lock() else {
            return CString::new("{\"ok\":false}").unwrap_or_default().into_raw();
        };
        g.groups
            .iter()
            .find(|grp| g_filter.is_empty() || grp.name == g_filter)
            .or_else(|| g.groups.first())
            .map(|grp| {
                grp.nodes
                    .iter()
                    .map(|n| {
                        let raw = g.raw_names.get(&n.name).cloned().unwrap_or_else(|| n.name.clone());
                        (n.name.clone(), raw)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let delays_map: HashMap<String, u32> = ctx.handle.block_on(async {
        let mut stream = stream::iter(nodes.into_iter().map(|(name, raw)| {
            let api = ctx.api.clone();
            async move {
                let d = api.delay(&raw).await.ok();
                (name, d)
            }
        }))
        .buffer_unordered(12);
        let mut results = HashMap::new();
        while let Some((name, d)) = stream.next().await {
            if let Some(ms) = d {
                results.insert(name, ms);
            }
        }
        results
    });
    if let Ok(mut g) = ctx.state.lock() {
        for (k, v) in &delays_map {
            let raw = g.raw_names.get(k).cloned().unwrap_or_else(|| k.clone());
            g.delays.insert(raw, *v);
        }
        for grp in &mut g.groups {
            for n in &mut grp.nodes {
                if let Some(&ms) = delays_map.get(&n.name) {
                    n.ms = ms;
                }
            }
        }
    }
    let out = serde_json::json!({
        "ok": true,
        "delays": delays_map,
    });
    CString::new(out.to_string()).unwrap_or_default().into_raw()
}

/// 常用网站真测（阻塞，Swift 在后台队列调用）。返回 `{"ok":true,"sites":[{id,name,domain,group,node,ms|null}]}`。
#[no_mangle]
pub extern "C" fn purr_core_probe_sites_json() -> *mut c_char {
    let Some(ctx) = CTX.get().cloned() else {
        return CString::new("{\"ok\":false}").unwrap_or_default().into_raw();
    };
    let mode = ctx.state.lock().map(|g| g.mode.clone()).unwrap_or_default();
    let port = crate::config::mixed_port();
    let sites = ctx
        .handle
        .block_on(async { crate::probe::probe_all(&ctx.api, port, &mode).await });
    let sites: Vec<_> = sites
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id, "name": p.name, "domain": p.domain,
                "group": p.group, "node": p.node, "ms": p.ms, "via": p.via_text(),
            })
        })
        .collect();
    let out = serde_json::json!({ "ok": true, "sites": sites });
    CString::new(out.to_string()).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn purr_core_import_subscription(url: *const c_char) -> *mut c_char {
    if url.is_null() {
        return CString::new("{\"ok\":false,\"error\":\"订阅地址为空\"}")
            .unwrap_or_default()
            .into_raw();
    }
    let raw_url = unsafe { CStr::from_ptr(url) }
        .to_string_lossy()
        .trim()
        .to_string();
    let Some(ctx) = CTX.get().cloned() else {
        return CString::new("{\"ok\":false,\"error\":\"内核未就绪\"}")
            .unwrap_or_default()
            .into_raw();
    };
    let result = ctx
        .handle
        .block_on(async { import_or_update_sub(&ctx, &raw_url, None).await });
    let json = match result {
        Ok((name, nodes)) => serde_json::json!({
            "ok": true,
            "name": name,
            "nodes": nodes,
        }),
        Err(err) => serde_json::json!({
            "ok": false,
            "error": err,
        }),
    };
    CString::new(json.to_string())
        .unwrap_or_default()
        .into_raw()
}

#[no_mangle]
pub extern "C" fn purr_core_use_subscription(id: u64) -> *mut c_char {
    let Some(ctx) = CTX.get().cloned() else {
        return CString::new("{\"ok\":false,\"error\":\"内核未就绪\"}")
            .unwrap_or_default()
            .into_raw();
    };
    let result: Result<String, String> = ctx.handle.block_on(async {
        let dir = crate::config::app_config_dir()?;
        let mut prefs = ctx
            .state
            .lock()
            .map_err(|_| "状态锁不可用")?
            .preferences
            .clone();
        let sub = prefs
            .subscriptions
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| "订阅不存在".to_string())?;
        let source = std::fs::read_to_string(dir.join("subscriptions").join(format!("{id}.yaml")))
            .map_err(|_| "没有找到该订阅缓存，请先点更新".to_string())?;
        let patched = prepare_yaml_text(&ctx.api, &prefs, &source)?;
        let path = dir.join("config.yaml");
        atomic_write(&path, patched.as_bytes())?;
        ctx.api.reload_path(&path).await?;
        prefs.subscription = Some(sub.url.clone());
        prefs.save(&dir)?;
        if let Ok(mut g) = ctx.state.lock() {
            g.preferences = prefs;
        }
        refresh_from_mihomo(&ctx.api, &ctx.state, 1).await;
        NEED_PLACE_REFRESH.store(true, Ordering::Relaxed);
        Ok(sub.name)
    });
    let json = match result {
        Ok(name) => serde_json::json!({"ok": true, "name": name}),
        Err(err) => serde_json::json!({"ok": false, "error": err}),
    };
    CString::new(json.to_string())
        .unwrap_or_default()
        .into_raw()
}

#[no_mangle]
pub extern "C" fn purr_core_update_subscription(id: u64) -> *mut c_char {
    let Some(ctx) = CTX.get().cloned() else {
        return CString::new("{\"ok\":false,\"error\":\"内核未就绪\"}")
            .unwrap_or_default()
            .into_raw();
    };
    let target_url = {
        let Ok(g) = ctx.state.lock() else {
            return CString::new("{\"ok\":false,\"error\":\"状态锁不可用\"}")
                .unwrap_or_default()
                .into_raw();
        };
        if id == 0 {
            g.preferences.subscription.clone()
        } else {
            g.preferences
                .subscriptions
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.url.clone())
        }
    };
    let Some(url) = target_url else {
        return CString::new("{\"ok\":false,\"error\":\"订阅不存在\"}")
            .unwrap_or_default()
            .into_raw();
    };
    let result = ctx.handle.block_on(async {
        import_or_update_sub(&ctx, &url, if id == 0 { None } else { Some(id) }).await
    });
    let json = match result {
        Ok((name, nodes)) => serde_json::json!({"ok": true, "name": name, "nodes": nodes}),
        Err(err) => serde_json::json!({"ok": false, "error": err}),
    };
    CString::new(json.to_string())
        .unwrap_or_default()
        .into_raw()
}

async fn import_or_update_sub(
    ctx: &Arc<RuntimeCtx>,
    url: &str,
    replace_id: Option<u64>,
) -> Result<(String, usize), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "订阅地址必须是 HTTP / HTTPS 链接")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("订阅地址必须是 HTTP / HTTPS 链接".into());
    }
    let direct_client = reqwest::Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .user_agent("clash.meta/Purr")
        .build()
        .map_err(|_| "无法创建订阅请求")?;
    let response = match direct_client.get(parsed.clone()).send().await {
        Ok(resp) => resp,
        Err(_) => {
            let fallback = reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .user_agent("clash.meta/Purr")
                .build()
                .map_err(|_| "无法创建订阅请求")?;
            fallback
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
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "订阅读取失败".to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("订阅配置超过 2 MB".into());
    }
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| "订阅不是 UTF-8 YAML 配置")?
        .trim_start_matches('\u{feff}')
        .to_string();

    let root: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|_| "订阅不是有效的 Clash YAML 配置".to_string())?;
    let map = root
        .as_mapping()
        .ok_or_else(|| "订阅不是有效的 Clash YAML 配置".to_string())?;
    if !map.contains_key(serde_yaml::Value::String("proxies".into()))
        && !map.contains_key(serde_yaml::Value::String("proxy-providers".into()))
    {
        return Err("订阅中没有找到节点 (proxies)".into());
    }

    let dir = crate::config::app_config_dir()?;
    let _ = crate::config::copy_geodata(&dir);
    let mut prefs = ctx
        .state
        .lock()
        .map_err(|_| "状态锁不可用")?
        .preferences
        .clone();
    let patched = prepare_yaml_text(&ctx.api, &prefs, &text)?;

    let id = replace_id.unwrap_or_else(|| {
        prefs
            .subscriptions
            .iter()
            .find(|s| s.url == url)
            .map(|s| s.id)
            .unwrap_or_else(|| prefs.subscriptions.iter().map(|s| s.id).max().unwrap_or(0) + 1)
    });
    let sub = crate::subscriptions::metadata(url, &headers, &text, id);
    let sub_name = sub.name.clone();
    let sub_nodes = sub.nodes;

    let cache_dir = dir.join("subscriptions");
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    atomic_write(&cache_dir.join(format!("{id}.yaml")), text.as_bytes())?;

    if let Some(slot) = prefs.subscriptions.iter_mut().find(|s| s.id == id) {
        *slot = sub;
    } else {
        prefs.subscriptions.push(sub);
    }
    prefs.subscription = Some(url.to_string());

    let cfg_path = dir.join("config.yaml");
    atomic_write(&cfg_path, patched.as_bytes())?;
    ctx.api.reload_path(&cfg_path).await?;
    prefs.save(&dir)?;

    if let Ok(mut g) = ctx.state.lock() {
        g.preferences = prefs;
    }
    refresh_from_mihomo(&ctx.api, &ctx.state, 1).await;
    NEED_PLACE_REFRESH.store(true, Ordering::Relaxed);
    Ok((sub_name, sub_nodes))
}

#[no_mangle]
pub extern "C" fn purr_core_close_all_connections() -> c_int {
    let Some(ctx) = CTX.get().cloned() else {
        return -1;
    };
    if let Ok(mut g) = ctx.state.lock() {
        g.connections.clear();
    }
    let handle = ctx.handle.clone();
    handle.spawn(async move {
        let _ = ctx.api.close_connections().await;
    });
    0
}

#[no_mangle]
pub extern "C" fn purr_core_close_connection(id: *const c_char) -> c_int {
    if id.is_null() {
        return -1;
    }
    let cid = unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned();
    let Some(ctx) = CTX.get().cloned() else {
        return -1;
    };
    if let Ok(mut g) = ctx.state.lock() {
        g.connections.retain(|c| c.id != cid);
    }
    let handle = ctx.handle.clone();
    handle.spawn(async move {
        let _ = ctx.api.close_connection(&cid).await;
    });
    0
}

#[no_mangle]
pub extern "C" fn purr_core_snapshot_json() -> *mut c_char {
    let Some(ctx) = CTX.get() else {
        return CString::new("{}").unwrap_or_default().into_raw();
    };
    let Ok(g) = ctx.state.lock() else {
        return CString::new("{}").unwrap_or_default().into_raw();
    };
    let now_epoch = crate::subscriptions::now();
    let active_url = g.preferences.subscription.as_deref();
    let subs: Vec<FfiSub> = g
        .preferences
        .subscriptions
        .iter()
        .map(|s| {
            let used_gb = s.used().map(|u| u / (1024 * 1024 * 1024));
            let total_gb = s.total.filter(|t| *t > 0).map(|t| (t / (1024 * 1024 * 1024)).max(1));
            let expires = s.expires.filter(|t| *t > 0);
            let exp = expires
                .map(crate::subscriptions::date)
                .unwrap_or_else(|| "未提供".into());
            let days_left = expires.map(|t| (t as i64 - now_epoch as i64).div_euclid(86400));
            let mins = now_epoch.saturating_sub(s.updated) / 60;
            let ago = if mins == 0 {
                "刚刚更新".to_string()
            } else if mins < 60 {
                format!("更新于 {mins} 分钟前")
            } else if mins < 1440 {
                format!("更新于 {} 小时前", mins / 60)
            } else {
                format!("更新于 {} 天前", mins / 1440)
            };
            FfiSub {
                id: s.id,
                name: s.name.clone(),
                url: s.url.clone(),
                desc: format!("{} 个窝点 · {ago}", s.nodes),
                used: used_gb,
                total: total_gb,
                exp,
                days_left,
                active: active_url == Some(s.url.as_str()),
                exhausted: s.exhausted(),
            }
        })
        .collect();

    let first_run = subs.is_empty() && g.groups.is_empty();
    let snap = FfiSnapshot {
        ready: g.ready,
        awake: IS_AWAKE.load(Ordering::Relaxed),
        first_run,
        mode: g.mode.clone(),
        tun: g.tun,
        dn_mb_s: g.dn_mb_s,
        up_mb_s: g.up_mb_s,
        proxy_mb: (g.proxy_bytes as f64) / (1024.0 * 1024.0),
        direct_mb: (g.direct_bytes as f64) / (1024.0 * 1024.0),
        blocked_count: g.blocked_count,
        mem_mb: g.mem_mb,
        exit_ip: g.exit_ip.clone(),
        exit_loc: g.exit_loc.clone(),
        direct_ip: g.direct_ip.clone(),
        direct_loc: g.direct_loc.clone(),
        groups: g.groups.clone(),
        subscriptions: subs,
        connections: g.connections.clone(),
        rules: g.rules.clone(),
    };
    let json = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
    CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn purr_core_check_update_json() -> *mut c_char {
    let Some(ctx) = CTX.get().cloned() else {
        let fallback = serde_json::json!({
            "ok": true,
            "current": env!("CARGO_PKG_VERSION"),
            "latest": env!("CARGO_PKG_VERSION"),
            "has_update": false,
            "url": "https://magpie.liujufu.dev/purr/"
        });
        return CString::new(fallback.to_string())
            .unwrap_or_default()
            .into_raw();
    };
    let res = ctx
        .handle
        .block_on(async { crate::status::check_remote_update().await });
    let json = match res {
        Ok(info) => serde_json::json!({
            "ok": true,
            "current": info.current,
            "latest": info.latest,
            "has_update": info.has_update,
            "url": info.url,
            "notes": info.notes,
        }),
        Err(e) => serde_json::json!({
            "ok": false,
            "current": env!("CARGO_PKG_VERSION"),
            "latest": env!("CARGO_PKG_VERSION"),
            "has_update": false,
            "url": "https://magpie.liujufu.dev/purr/",
            "error": e,
        }),
    };
    CString::new(json.to_string())
        .unwrap_or_default()
        .into_raw()
}

#[no_mangle]
pub extern "C" fn purr_core_load_preferences_json() -> *mut c_char {
    let dir = crate::config::app_config_dir().unwrap_or_else(|_| std::env::temp_dir());
    let prefs = Preferences::load(&dir).unwrap_or_default();
    let json = serde_json::to_string(&prefs).unwrap_or_else(|_| "{}".into());
    CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn purr_core_update_preference(
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    if key.is_null() || value.is_null() {
        return -1;
    }
    let k = unsafe { CStr::from_ptr(key) }.to_string_lossy().into_owned();
    let v = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
    let Ok(dir) = crate::config::app_config_dir() else {
        return -1;
    };
    let _ = std::fs::create_dir_all(&dir);
    let current = if let Some(ctx) = CTX.get() {
        ctx.state
            .lock()
            .map(|s| s.preferences.clone())
            .unwrap_or_else(|_| Preferences::load(&dir).unwrap_or_default())
    } else {
        Preferences::load(&dir).unwrap_or_default()
    };
    let Ok(next) = current.changed(&k, &v) else {
        return -2;
    };
    if next.save(&dir).is_err() {
        return -3;
    }
    let old_port = current.mixed_port;
    let old_sys = current.system_proxy;
    crate::config::set_mixed_port(next.mixed_port);
    let cfg_path = dir.join("config.yaml");
    if let Ok(raw) = std::fs::read_to_string(&cfg_path) {
        if let Ok(patched) = yaml_setting(&raw, &next, &k) {
            if patched != raw {
                let _ = atomic_write(&cfg_path, patched.as_bytes());
            }
        }
    }
    if k == "auto-wake" {
        let _ = crate::settings::set_autostart(next.auto_wake);
    }
    if let Some(ctx) = CTX.get().cloned() {
        if let Ok(mut g) = ctx.state.lock() {
            g.preferences = next.clone();
        }
        let awake = IS_AWAKE.load(Ordering::Relaxed);
        let handle = ctx.handle.clone();
        handle.spawn(async move {
            let _ = ctx.api.reload_path(&cfg_path).await;
            if matches!(k.as_str(), "mixed-port" | "allow-lan" | "ipv6") {
                let _ = ctx
                    .api
                    .patch_configs(serde_json::json!({
                        "mixed-port": next.mixed_port,
                        "allow-lan": next.allow_lan,
                        "ipv6": next.ipv6,
                    }))
                    .await;
            }
            if awake && next.system_proxy && (next.mixed_port != old_port || !old_sys) {
                let _ = crate::proxy::enable();
            } else if awake && old_sys && !next.system_proxy {
                crate::proxy::restore();
            }
        });
    }
    0
}

#[no_mangle]
pub extern "C" fn purr_core_set_system_proxy(enabled: bool) -> c_int {
    if enabled {
        match crate::proxy::enable() {
            Ok(()) => 0,
            Err(_) => -1,
        }
    } else {
        crate::proxy::restore();
        0
    }
}

#[no_mangle]
pub extern "C" fn purr_core_flush_dns() -> c_int {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("/usr/bin/dscacheutil")
            .arg("-flushcache")
            .status();
        if status.is_ok_and(|s| s.success()) {
            return 0;
        }
        return -1;
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

/// True when the core can create a utun device (macOS: authorised setuid copy, or Purr runs as root).
#[no_mangle]
pub extern "C" fn purr_core_tun_authorized() -> bool {
    #[cfg(unix)]
    if unsafe { libc::geteuid() } == 0 {
        return true;
    }
    crate::config::authorized_core().is_some()
}

/// Restart mihomo from `config::launch_binary()` (after TUN authorisation), then re-apply the mode.
/// Returns 0 once the controller answers again.
#[no_mangle]
pub extern "C" fn purr_core_restart_core() -> c_int {
    let Some(ctx) = CTX.get().cloned() else {
        return -1;
    };
    let Ok(dir) = crate::config::app_config_dir() else {
        return -1;
    };
    let binary = crate::config::launch_binary();
    let old = ctx._core.lock().ok().and_then(|mut g| g.take());
    let api = ctx.api.clone();
    let mode = ctx.state.lock().map(|g| g.mode.clone()).unwrap_or_else(|_| "rule".into());
    let (core, ready) = ctx.handle.block_on(async move {
        if let Some(core) = old {
            core.shutdown().await;
        }
        let core = Core::start(binary, dir);
        let ready = tokio::time::timeout(Duration::from_secs(20), api.wait_ready()).await;
        if matches!(ready, Ok(Ok(()))) && !mode.is_empty() {
            let _ = api.set_mode(&mode).await;
        }
        (core, matches!(ready, Ok(Ok(()))))
    });
    if let Ok(mut g) = ctx._core.lock() {
        *g = Some(core);
    }
    if let Ok(mut g) = ctx.state.lock() {
        g.tun = false;
    }
    if ready { 0 } else { -1 }
}

#[no_mangle]
pub extern "C" fn purr_core_shutdown() {
    crate::proxy::restore();
    crate::core::terminate_running();
}

#[no_mangle]
pub extern "C" fn purr_core_version() -> *mut c_char {
    CString::new(env!("CARGO_PKG_VERSION"))
        .unwrap_or_default()
        .into_raw()
}

#[no_mangle]
pub extern "C" fn purr_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}
