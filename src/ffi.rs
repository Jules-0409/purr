// SPDX-License-Identifier: GPL-3.0-only

//! C-ABI FFI bindings for macOS SwiftUI native frontend to drive Purr's real Rust + mihomo engine.
// The exported C functions take raw pointers from Swift by design; null checks are done inside.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

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
    /// The protocol it really connects with; for a nested group, the protocol of its current node.
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
    /// Decimal GB (10^9 B, as providers print quotas) from the subscription-userinfo header;
    /// null when the provider does not send it.
    pub used: Option<f64>,
    pub total: Option<f64>,
    /// Expiry date on the local clock, or "未提供".
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
    /// Seconds since the connection started; null when its start time did not parse.
    pub age_s: Option<u64>,
    /// Same text as the Windows connections page ("2m05s", "—").
    pub age: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FfiRule {
    pub kind: String,
    pub match_str: String,
    pub policy: String,
    /// The core's match count since it started (`extra.hitCount`); null when not reported.
    pub hits: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FfiSnapshot {
    pub ready: bool,
    pub awake: bool,
    pub first_run: bool,
    pub mode: String,
    pub tun: bool,
    /// Decimal MB/s.
    pub dn_mb_s: f64,
    pub up_mb_s: f64,
    /// Today's traffic on the local calendar day (persisted in statistics.json, same counting as
    /// Windows); null until the first successful /connections poll of this run.
    pub today_down: Option<String>,
    pub today_up: Option<String>,
    pub today_total: Option<String>,
    pub route_proxy: Option<String>,
    pub route_direct: Option<String>,
    /// Proxy share of today's routed bytes (0–1); null when nothing was routed yet.
    pub route_proxy_fraction: Option<f64>,
    /// Rule-rejected TCP connections today; null until the log stream has connected.
    pub blocked_count: Option<u64>,
    /// Decimal MB; null until both processes were measured.
    pub mem_mb: Option<f64>,
    pub mem_shell_mb: Option<f64>,
    pub mem_core_mb: Option<f64>,
    /// CPU of Purr + core (100 % = one core, like Activity Monitor); null until two samples.
    pub cpu_pct: Option<f64>,
    pub exit_ip: String,
    pub exit_loc: String,
    /// "pending" (looking up), "ok", "failed".
    pub exit_state: String,
    pub direct_ip: String,
    pub direct_loc: String,
    pub direct_state: String,
    /// "", "timeout", "offline", "sub" — same detection as the Windows health loop.
    pub trouble: String,
    pub version: String,
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
    telemetry: crate::telemetry::Telemetry,
    stats_ready: bool,
    log_ready: bool,
    mem_shell: Option<u64>,
    mem_core: Option<u64>,
    cpu_pct: Option<f64>,
    cpu: crate::status::CpuMeter,
    exit_ip: String,
    exit_loc: String,
    exit_state: String,
    direct_ip: String,
    direct_loc: String,
    direct_state: String,
    trouble: String,
    groups: Vec<FfiGroup>,
    /// The core's groups as parsed, for picking the main group and health checks.
    raw_groups: Vec<crate::present::ProxyGroup>,
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
            telemetry: Default::default(),
            stats_ready: false,
            log_ready: false,
            mem_shell: None,
            mem_core: None,
            cpu_pct: None,
            cpu: Default::default(),
            exit_ip: "—".into(),
            exit_loc: String::new(),
            exit_state: "pending".into(),
            direct_ip: "—".into(),
            direct_loc: String::new(),
            direct_state: "pending".into(),
            trouble: String::new(),
            groups: Vec::new(),
            raw_groups: Vec::new(),
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

/// Whether the first-launch notice was accepted. Safe to call before `purr_core_init`.
#[no_mangle]
pub extern "C" fn purr_core_disclaimer_accepted() -> bool {
    crate::config::app_config_dir()
        .map(|dir| crate::settings::disclaimer_accepted(&dir))
        .unwrap_or(false)
}

/// Checks the typed phrase and records the acceptance. 0 = saved, -1 = wrong phrase, -2 = write failed.
#[no_mangle]
pub extern "C" fn purr_core_accept_disclaimer(typed: *const c_char) -> c_int {
    if typed.is_null() {
        return -1;
    }
    let typed = unsafe { CStr::from_ptr(typed) }.to_string_lossy();
    if !crate::settings::disclaimer_input_matches(&typed) {
        return -1;
    }
    match crate::config::app_config_dir().and_then(|dir| crate::settings::accept_disclaimer(&dir)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("purr_core_accept_disclaimer: {e}");
            -2
        }
    }
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
        telemetry: crate::telemetry::Telemetry::load(&settings.config_dir),
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
        let sys = state
            .lock()
            .map(|s| s.preferences.system_proxy)
            .unwrap_or(true);
        if sys {
            let _ = crate::proxy::enable();
        }
    }
    tokio::spawn(direct_place_loop(Arc::clone(&state)));
    tokio::spawn(reject_log_loop(api.clone(), Arc::clone(&state)));

    let mut last_place_check = Instant::now() - Duration::from_secs(60);
    let mut last_save = Instant::now();
    let mut last_debug = Instant::now() - Duration::from_secs(60);
    let mut health = Health::default();
    let mut tick_count = 0u64;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        tick_count = tick_count.wrapping_add(1);
        refresh_from_mihomo(&api, &state, tick_count).await;
        health_tick(&api, &state, &mut health).await;

        let awake = IS_AWAKE.load(Ordering::Relaxed);
        let need_place = NEED_PLACE_REFRESH.swap(false, Ordering::Relaxed);
        if need_place {
            // The node, mode or subscription changed: the old exit IP is no longer true.
            if let Ok(mut g) = state.lock() {
                g.exit_ip = "—".into();
                g.exit_loc = String::new();
                g.exit_state = "pending".into();
            }
        }
        if awake && (need_place || last_place_check.elapsed() >= Duration::from_secs(20)) {
            last_place_check = Instant::now();
            let state_place = Arc::clone(&state);
            tokio::spawn(async move {
                let place = crate::status::lookup_place().await;
                let Ok(mut g) = state_place.lock() else {
                    return;
                };
                if place == "暂时不知道" {
                    g.exit_ip = "—".into();
                    g.exit_loc = String::new();
                    g.exit_state = "failed".into();
                } else {
                    let (ip, loc) = match place.split_once(" · ") {
                        Some((i, l)) => (i.trim().to_string(), l.trim().to_string()),
                        None => (place, String::new()),
                    };
                    g.exit_ip = ip;
                    g.exit_loc = loc;
                    g.exit_state = "ok".into();
                }
            });
        }
        if last_save.elapsed() >= Duration::from_secs(60) {
            last_save = Instant::now();
            save_telemetry(&state);
        }
        if last_debug.elapsed() >= Duration::from_secs(10) {
            last_debug = Instant::now();
            write_debug_values(&state);
        }
    }
}

fn save_telemetry(state: &Arc<Mutex<FfiState>>) {
    let Ok(dir) = crate::config::app_config_dir() else {
        return;
    };
    if let Ok(mut g) = state.lock() {
        g.telemetry.roll(crate::subscriptions::now());
        if let Err(e) = g.telemetry.save(&dir) {
            eprintln!("purr: statistics not saved: {e}");
        }
    }
}

/// Off by default. When `debug-values` exists in the config dir, every 10 s the values the UI is
/// given are written to `debug-snapshot.json` and one summary line is appended to
/// `debug-values.log`, so they can be compared with the core API and `ps`.
fn write_debug_values(_state: &Arc<Mutex<FfiState>>) {
    let Ok(dir) = crate::config::app_config_dir() else {
        return;
    };
    if !dir.join("debug-values").exists() {
        return;
    }
    let json = snapshot_string();
    let _ = std::fs::write(dir.join("debug-snapshot.json"), &json);
    let Ok(v) = serde_json::from_str::<Value>(&json) else {
        return;
    };
    let line = serde_json::json!({
        "t": crate::subscriptions::now(),
        "conns": v["connections"].as_array().map(Vec::len),
        "dn_mb_s": v["dn_mb_s"], "up_mb_s": v["up_mb_s"],
        "today": v["today_total"], "down": v["today_down"], "up": v["today_up"],
        "blocked": v["blocked_count"], "mem_mb": v["mem_mb"],
        "mem_shell_mb": v["mem_shell_mb"], "mem_core_mb": v["mem_core_mb"],
        "cpu_pct": v["cpu_pct"], "exit": v["exit_ip"], "exit_state": v["exit_state"],
        "direct": v["direct_ip"], "trouble": v["trouble"],
        "ages": v["connections"].as_array().map(|a| a.iter().take(5).map(|c| c["age_s"].clone()).collect::<Vec<_>>()),
        "hits": v["rules"].as_array().map(|a| a.iter().filter(|r| r["hits"].as_u64().unwrap_or(0) > 0).map(|r| format!("{}:{}={}", r["kind"].as_str().unwrap_or(""), r["match_str"].as_str().unwrap_or(""), r["hits"])).collect::<Vec<_>>()),
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("debug-values.log"))
    {
        let _ = writeln!(f, "{line}");
    }
}

#[derive(Default)]
struct Health {
    failed: u8,
    previous: String,
    last_probe: Option<Instant>,
}

/// Same rules as the Windows `health_loop`: an exhausted active subscription is "sub"; awake with
/// no routable address is "offline"; three failed delay probes (20 s apart) of the main group's
/// current member is "timeout". The fixture marker used by the validation suite turns off the
/// offline check, as on Windows.
async fn health_tick(api: &Api, state: &Arc<Mutex<FfiState>>, h: &mut Health) {
    let awake = IS_AWAKE.load(Ordering::Relaxed);
    let fixture =
        crate::config::app_config_dir().is_ok_and(|d| d.join("purr-validation-fixture").exists());
    let target = {
        let Ok(mut g) = state.lock() else {
            return;
        };
        let exhausted = g
            .preferences
            .subscriptions
            .iter()
            .find(|sub| Some(&sub.url) == g.preferences.subscription.as_ref())
            .is_some_and(|sub| sub.exhausted());
        if exhausted {
            g.trouble = "sub".into();
        } else if g.trouble == "sub" {
            g.trouble.clear();
        }
        if !awake {
            if g.trouble == "offline" || g.trouble == "timeout" {
                g.trouble.clear();
            }
        } else if !fixture {
            let online = crate::status::network_available();
            if !online && (g.trouble.is_empty() || g.trouble == "offline") {
                g.trouble = "offline".into();
            } else if online && g.trouble == "offline" {
                g.trouble.clear();
            }
        }
        if !awake
            || g.mode == "direct"
            || !g.ready
            || (!g.trouble.is_empty() && g.trouble != "timeout")
        {
            None
        } else {
            crate::present::main_group(&g.raw_groups, &g.mode).and_then(|grp| {
                let member = grp.nodes.iter().find(|n| n.name == grp.now)?;
                crate::present::probe_delay(&member.kind).then(|| grp.now.clone())
            })
        }
    };
    let Some(now) = target else {
        h.failed = 0;
        return;
    };
    if now != h.previous {
        h.failed = 0;
        h.previous = now.clone();
    }
    if h.last_probe
        .is_some_and(|t| t.elapsed() < Duration::from_secs(20))
    {
        return;
    }
    h.last_probe = Some(Instant::now());
    match api.delay(&now).await {
        Ok(ms) => {
            h.failed = 0;
            if let Ok(mut g) = state.lock() {
                g.delays.insert(now.clone(), ms);
                if g.trouble == "timeout" {
                    g.trouble.clear();
                }
            }
        }
        Err(_) => {
            h.failed = h.failed.saturating_add(1);
            if h.failed >= 3 {
                h.failed = 0;
                if let Ok(mut g) = state.lock() {
                    g.trouble = "timeout".into();
                }
            }
        }
    }
}

/// Counts rule-rejected connections (the 抓老鼠 number) from the core's info log into today's
/// statistics. Until the stream has connected once the count is unknown ("—").
async fn reject_log_loop(api: Api, state: Arc<Mutex<FfiState>>) {
    use tokio_tungstenite::tungstenite::Message;
    loop {
        if let Ok(mut socket) = api.websocket("/logs?level=info").await {
            if let Ok(mut g) = state.lock() {
                g.log_ready = true;
            }
            while let Some(Ok(message)) = socket.next().await {
                let text = match message {
                    Message::Text(t) => t.to_string(),
                    Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
                    _ => continue,
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let payload = value.get("payload").and_then(Value::as_str).unwrap_or("");
                if crate::present::is_reject_log(payload) {
                    if let Ok(mut g) = state.lock() {
                        g.telemetry.reject(crate::subscriptions::now());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// The local (direct) exit: looked up at start, retried with backoff (5 s doubling to 5 min) while
/// it fails, re-checked every 10 min once known. Shows "—" after a failure, never a stale guess.
async fn direct_place_loop(state: Arc<Mutex<FfiState>>) {
    let mut backoff = Duration::from_secs(5);
    loop {
        let found = lookup_direct_place().await;
        let wait = {
            let Ok(mut g) = state.lock() else {
                return;
            };
            match found {
                Some((ip, loc)) => {
                    g.direct_ip = ip;
                    g.direct_loc = loc;
                    g.direct_state = "ok".into();
                    backoff = Duration::from_secs(5);
                    Duration::from_secs(600)
                }
                None => {
                    g.direct_ip = "—".into();
                    g.direct_loc = String::new();
                    g.direct_state = "failed".into();
                    let w = backoff;
                    backoff = (backoff * 2).min(Duration::from_secs(300));
                    w
                }
            }
        };
        tokio::time::sleep(wait).await;
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
    // Memory and CPU every 3 s: a root (TUN) core can only be read through `ps`, and spawning it
    // every second would show up in Purr's own CPU.
    if tick_count % 3 == 1 {
        let shell_pid = std::process::id();
        let core_pid = crate::core::running_pid();
        let shell = crate::status::process_sample(shell_pid);
        let core = crate::status::process_sample(core_pid);
        if let Ok(mut g) = state.lock() {
            g.mem_shell = shell.memory;
            g.mem_core = core.memory;
            let total = shell.cpu.zip(core.cpu).map(|(a, b)| a + b);
            g.cpu_pct = g.cpu.update(&[shell_pid, core_pid], total);
        }
    }

    let mode = match api.mode().await {
        Ok(m) => m,
        Err(_) => {
            if let Ok(mut g) = state.lock() {
                g.ready = false;
            }
            return;
        }
    };

    let proxies_json = api.proxies().await.ok();
    let conns_json = api.connections().await.ok();
    let need_rules =
        state.lock().map(|g| g.rules.is_empty()).unwrap_or(true) || tick_count % 10 == 1;
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

    if let Some(ref pjson) = proxies_json {
        let mut parsed = crate::present::proxy_groups(pjson);
        g.raw_groups = parsed.clone();
        // 手选的 select 组排最前（托盘/概览默认操作第一组），fallback/url-test 等自动组靠后，GLOBAL 仍在最后
        let main = crate::present::main_group(&parsed, &g.mode).map(|m| m.name.clone());
        parsed.sort_by_key(|pg| {
            if Some(&pg.name) == main.as_ref() {
                0
            } else if pg.name == "GLOBAL" {
                3
            } else if pg.kind == "select" {
                1
            } else {
                2
            }
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
                        proto: crate::present::leaf_kind(pjson, &n.name)
                            .unwrap_or_else(|| n.kind.clone()),
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
        g.groups = next_groups;
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
            g.dn_mb_s = dn_tot.saturating_sub(prev_dn) as f64 / crate::present::MB / dt;
            g.up_mb_s = up_tot.saturating_sub(prev_up) as f64 / crate::present::MB / dt;
        }
        g.last_dn_total = Some(dn_tot);
        g.last_up_total = Some(up_tot);
        g.last_rate_at = Some(now_inst);
        g.telemetry.connections(cjson, crate::subscriptions::now());
        g.stats_ready = true;

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
                let is_reject = first_chain.to_ascii_uppercase().starts_with("REJECT")
                    || last_chain.to_ascii_uppercase().starts_with("REJECT");

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
                let rule_type = item.get("rule").and_then(Value::as_str).unwrap_or("—");
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
                next_conns.push(FfiConn {
                    id,
                    app,
                    host,
                    rule,
                    pol,
                    chain: chain_display,
                    dn_kb: down as f64 / crate::present::KB,
                    up_kb: up as f64 / crate::present::KB,
                    age_s: crate::present::age_secs(start_str, now_sys),
                    age: crate::present::age_text(start_str, now_sys),
                });
            }
        }
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
                        .unwrap_or("—")
                        .to_uppercase(),
                    match_str: r
                        .get("payload")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("*")
                        .to_string(),
                    policy: crate::present::display_name(
                        r.get("proxy").and_then(Value::as_str).unwrap_or("—"),
                    ),
                    hits: crate::present::rule_hits(r),
                })
                .collect();
        }
    }
}

/// Picks the fastest other member of the main select group (real delay tests) and switches to it;
/// the Windows `select_available`. Returns the new member's display name.
async fn swap_to_best(ctx: &Arc<RuntimeCtx>) -> Result<String, String> {
    let (group, now, names) = {
        let g = ctx.state.lock().map_err(|_| "状态锁不可用")?;
        let grp = crate::present::main_group(&g.raw_groups, &g.mode)
            .filter(|grp| grp.kind == "select")
            .or_else(|| g.raw_groups.iter().find(|grp| grp.kind == "select"))
            .ok_or("没有可切换的节点组")?;
        let names: Vec<String> = grp
            .nodes
            .iter()
            .filter(|n| n.name != grp.now && crate::present::probe_delay(&n.kind))
            .map(|n| n.name.clone())
            .collect();
        (grp.name.clone(), grp.now.clone(), names)
    };
    let mut probes = stream::iter(names.into_iter().map(|name| {
        let api = ctx.api.clone();
        async move {
            let d = api.delay(&name).await.ok();
            (name, d)
        }
    }))
    .buffer_unordered(4);
    let mut best: Option<(String, u32)> = None;
    while let Some((name, d)) = probes.next().await {
        if let Some(ms) = d {
            if let Ok(mut g) = ctx.state.lock() {
                g.delays.insert(name.clone(), ms);
            }
            if best.as_ref().is_none_or(|(_, b)| ms < *b) {
                best = Some((name, ms));
            }
        }
    }
    let (name, _) = best.ok_or("这个组暂时没有可用的窝点，请更新订阅或重试")?;
    let unchanged = ctx
        .state
        .lock()
        .map(|g| {
            g.raw_groups
                .iter()
                .any(|grp| grp.name == group && grp.now == now)
        })
        .unwrap_or(false);
    if !unchanged {
        return Err("节点已由其他操作切换".into());
    }
    ctx.api.select(&group, &name).await?;
    if let Ok(mut g) = ctx.state.lock() {
        g.trouble.clear();
    }
    NEED_PLACE_REFRESH.store(true, Ordering::Relaxed);
    Ok(crate::present::display_name(&name))
}

/// Acts on the current trouble for real: "swap" switches to the fastest working member,
/// "retry" re-checks (a delay probe of the current member, or the network) and only clears the
/// trouble when that check passes. Returns `{"ok":bool,"message":...}`.
#[no_mangle]
pub extern "C" fn purr_core_fix_trouble_json(action: *const c_char) -> *mut c_char {
    let action = if action.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(action) }
            .to_string_lossy()
            .into_owned()
    };
    let out = match CTX.get().cloned() {
        None => Err("内核未就绪".to_string()),
        Some(ctx) => {
            let trouble = ctx
                .state
                .lock()
                .map(|g| g.trouble.clone())
                .unwrap_or_default();
            let handle = ctx.handle.clone();
            handle.block_on(async {
                match (action.as_str(), trouble.as_str()) {
                    ("swap", _) => swap_to_best(&ctx).await.map(|n| format!("已换到 {n}")),
                    (_, "offline") => {
                        if crate::status::network_available() {
                            if let Ok(mut g) = ctx.state.lock() {
                                g.trouble.clear();
                            }
                            Ok("网络回来了 · 继续呼噜".to_string())
                        } else {
                            Err("网络还没有恢复，请先连接 Wi-Fi 或有线网络".to_string())
                        }
                    }
                    _ => {
                        let now = ctx.state.lock().ok().and_then(|g| {
                            crate::present::main_group(&g.raw_groups, &g.mode)
                                .map(|grp| grp.now.clone())
                        });
                        let now = now.ok_or("没有可测速的窝点".to_string())?;
                        let ms = ctx
                            .api
                            .delay(&now)
                            .await
                            .map_err(|_| "它还没有回应，可以换一个窝点".to_string())?;
                        if let Ok(mut g) = ctx.state.lock() {
                            g.delays.insert(now, ms);
                            if g.trouble == "timeout" {
                                g.trouble.clear();
                            }
                        }
                        Ok(format!("它回应了 · {ms}ms · 继续呼噜"))
                    }
                }
            })
        }
    };
    let json = match out {
        Ok(m) => serde_json::json!({"ok": true, "message": m}),
        Err(e) => serde_json::json!({"ok": false, "message": e}),
    };
    CString::new(json.to_string())
        .unwrap_or_default()
        .into_raw()
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
                g.exit_ip = "—".into();
                g.exit_loc = String::new();
                g.exit_state = "pending".into();
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn purr_core_set_mode(mode: *const c_char) {
    if mode.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(mode) }
        .to_string_lossy()
        .into_owned();
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
        return CString::new("{\"ok\":false}")
            .unwrap_or_default()
            .into_raw();
    };
    // (显示名, 真实名)：测速要用 mihomo 里的真实名字，结果仍按显示名回给界面
    let nodes: Vec<(String, String)> = {
        let Ok(g) = ctx.state.lock() else {
            return CString::new("{\"ok\":false}")
                .unwrap_or_default()
                .into_raw();
        };
        g.groups
            .iter()
            .find(|grp| g_filter.is_empty() || grp.name == g_filter)
            .or_else(|| g.groups.first())
            .map(|grp| {
                grp.nodes
                    .iter()
                    .map(|n| {
                        let raw = g
                            .raw_names
                            .get(&n.name)
                            .cloned()
                            .unwrap_or_else(|| n.name.clone());
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
        return CString::new("{\"ok\":false}")
            .unwrap_or_default()
            .into_raw();
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
    CString::new(snapshot_string())
        .unwrap_or_default()
        .into_raw()
}

fn snapshot_string() -> String {
    let Some(ctx) = CTX.get() else {
        return "{}".into();
    };
    let Ok(g) = ctx.state.lock() else {
        return "{}".into();
    };
    snapshot_of(&g).unwrap_or_else(|| "{}".into())
}

fn snapshot_of(g: &FfiState) -> Option<String> {
    use crate::present::{size_text, GB, MB};
    let now_epoch = crate::subscriptions::now();
    let active_url = g.preferences.subscription.as_deref();
    let subs: Vec<FfiSub> = g
        .preferences
        .subscriptions
        .iter()
        .map(|s| {
            let expires = s.expires.filter(|t| *t > 0);
            let exp = expires
                .map(crate::subscriptions::date)
                .unwrap_or_else(|| "未提供".into());
            let days_left = expires.map(|t| (t as i64 - now_epoch as i64).div_euclid(86400));
            let mins = now_epoch.saturating_sub(s.updated) / 60;
            let ago = if s.updated == 0 {
                "尚未更新".to_string()
            } else if mins == 0 {
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
                used: s.used().map(|u| u as f64 / GB),
                total: s.total.filter(|t| *t > 0).map(|t| t as f64 / GB),
                exp,
                days_left,
                active: active_url == Some(s.url.as_str()),
                exhausted: s.exhausted(),
            }
        })
        .collect();

    let t = &g.telemetry;
    let ready_text = |bytes: u64| g.stats_ready.then(|| size_text(bytes));
    let routed = t.routes[0].saturating_add(t.routes[1]);
    let mb = |b: Option<u64>| b.map(|b| b as f64 / MB);
    let first_run = subs.is_empty() && g.groups.is_empty();
    let snap = FfiSnapshot {
        ready: g.ready,
        awake: IS_AWAKE.load(Ordering::Relaxed),
        first_run,
        mode: g.mode.clone(),
        tun: g.tun,
        dn_mb_s: g.dn_mb_s,
        up_mb_s: g.up_mb_s,
        today_down: ready_text(t.down),
        today_up: ready_text(t.up),
        today_total: ready_text(t.down.saturating_add(t.up)),
        route_proxy: ready_text(t.routes[0]),
        route_direct: ready_text(t.routes[1]),
        route_proxy_fraction: (g.stats_ready && routed > 0)
            .then(|| t.routes[0] as f64 / routed as f64),
        blocked_count: g.log_ready.then_some(t.blocked),
        mem_mb: mb(g.mem_shell.zip(g.mem_core).map(|(a, b)| a + b)),
        mem_shell_mb: mb(g.mem_shell),
        mem_core_mb: mb(g.mem_core),
        cpu_pct: g.cpu_pct,
        exit_ip: g.exit_ip.clone(),
        exit_loc: g.exit_loc.clone(),
        exit_state: g.exit_state.clone(),
        direct_ip: g.direct_ip.clone(),
        direct_loc: g.direct_loc.clone(),
        direct_state: g.direct_state.clone(),
        trouble: g.trouble.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        groups: g.groups.clone(),
        subscriptions: subs,
        connections: g.connections.clone(),
        rules: g.rules.clone(),
    };
    serde_json::to_string(&snap).ok()
}

#[no_mangle]
pub extern "C" fn purr_core_check_update_json() -> *mut c_char {
    let Some(ctx) = CTX.get().cloned() else {
        let fallback = serde_json::json!({
            "ok": false,
            "current": env!("CARGO_PKG_VERSION"),
            "error": "内核未就绪",
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
pub extern "C" fn purr_core_update_preference(key: *const c_char, value: *const c_char) -> c_int {
    if key.is_null() || value.is_null() {
        return -1;
    }
    let k = unsafe { CStr::from_ptr(key) }
        .to_string_lossy()
        .into_owned();
    let v = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
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
            0
        } else {
            -1
        }
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
    let mode = ctx
        .state
        .lock()
        .map(|g| g.mode.clone())
        .unwrap_or_else(|_| "rule".into());
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
    if ready {
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn purr_core_shutdown() {
    if let Some(ctx) = CTX.get() {
        save_telemetry(&ctx.state);
    }
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
