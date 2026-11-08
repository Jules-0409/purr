// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyNode {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub delay_ms: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyGroup {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub now: String,
    pub now_label: String,
    pub nodes: Vec<ProxyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conn {
    pub id: String,
    pub app: String,
    pub host: String,
    pub rule: String,
    pub policy: String,
    pub kind: i32,
    pub down: String,
    pub up: String,
    pub age: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totals {
    pub down: String,
    pub up: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub index: String,
    pub kind: String,
    pub payload: String,
    pub policy: String,
    pub tone: i32,
    /// The core's own match count for this rule since it started (`extra.hitCount`); None when the
    /// core does not report it.
    pub hits: Option<u64>,
}

pub struct Wave {
    pub main: String,
    pub warm: String,
}

pub struct Chart {
    pub fill: String,
    pub down: String,
    pub up: String,
    pub grid: String,
    pub peak: String,
}

#[allow(dead_code)]
const EAR: [(f32, f32); 7] = [
    (-50.0, 0.0),
    (-33.0, -46.0),
    (-15.0, -19.0),
    (0.0, -22.0),
    (15.0, -19.0),
    (33.0, -46.0),
    (50.0, 0.0),
];

pub fn display_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if is_emoji(ch) {
            continue;
        }
        out.push(ch);
    }
    let label = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if label.is_empty() {
        name.trim().to_string()
    } else {
        label
    }
}

pub fn short_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = text.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    text
}

pub fn clock_now() -> String {
    #[cfg(unix)]
    {
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        let mut local = unsafe { std::mem::zeroed::<libc::tm>() };
        if unsafe { libc::localtime_r(&now, &mut local) }.is_null() {
            return "--:--:--".to_string();
        }
        format!(
            "{:02}:{:02}:{:02}",
            local.tm_hour, local.tm_min, local.tm_sec
        )
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLocalTime(time: *mut u16);
        }
        let mut time = [0u16; 8];
        unsafe {
            GetLocalTime(time.as_mut_ptr());
        }
        format!("{:02}:{:02}:{:02}", time[4], time[5], time[6])
    }
    #[cfg(not(any(unix, windows)))]
    {
        "--:--:--".into()
    }
}

pub fn format_hms(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

/// One decimal (SI) unit rule for every size and speed Purr shows: 1 KB = 1000 B, 1 MB = 10^6 B,
/// 1 GB = 10^9 B. It matches what subscription providers print for their quotas.
pub const KB: f64 = 1e3;
pub const MB: f64 = 1e6;
pub const GB: f64 = 1e9;

/// Speed for display. Below 1 MB/s it switches to KB/s: one decimal of MB/s rounds every
/// ordinary browsing rate (under ~50 KB/s) down to "0.0".
pub fn rate_parts(bytes: u64) -> (String, String) {
    if bytes as f64 >= MB {
        (format!("{:.1}", bytes as f64 / MB), "MB/s".into())
    } else {
        (format!("{:.0}", bytes as f64 / KB), "KB/s".into())
    }
}
pub fn rate_text(bytes: u64) -> String {
    let (value, unit) = rate_parts(bytes);
    format!("{value} {unit}")
}
/// mihomo logs every rule-rejected connection at info level ("... match X using REJECT").
/// Rejected connections are closed before they are tracked, so `/connections` never shows them.
///
/// UDP rejects are left out: they are mostly QUIC (UDP 443) being pushed back to TCP, retried
/// many times per page, and would swamp the ads/trackers count.
pub fn is_reject_log(payload: &str) -> bool {
    payload.contains(" using REJECT") && !payload.starts_with("[UDP]")
}
pub fn size_text(bytes: u64) -> String {
    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.0} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

/// A quota in whole decimal GB as providers print it; one decimal below 10 GB so a 2 GB trial plan
/// with 0.4 GB used is not shown as "0 / 2" (or rounded up to 1).
pub fn gb_text(bytes: u64) -> String {
    let gb = bytes as f64 / GB;
    if gb < 10.0 {
        format!("{gb:.1}")
    } else {
        format!("{gb:.0}")
    }
}

/// Memory in decimal MB (same unit as every other size here).
pub fn mb_text(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / MB)
}

pub fn place_from_json(value: &Value) -> Option<String> {
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    if value.get("status").and_then(Value::as_str) == Some("fail") {
        return None;
    }
    if value
        .get("status")
        .and_then(Value::as_u64)
        .is_some_and(|code| code >= 400)
    {
        return None;
    }
    let ip = first_text(value, &["ip", "query"]);
    if ip.is_empty() {
        return None;
    }
    let city = first_text(value, &["city"]);
    let country = first_text(
        value,
        &["country_code", "countryCode", "country_iso", "country"],
    );
    Some(crate::status::format_place(&ip, &city, &country))
}

pub fn proxy_groups(root: &Value) -> Vec<ProxyGroup> {
    let Some(proxies) = root.get("proxies").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    let mut global = None;
    for (name, item) in proxies {
        let raw_kind = item.get("type").and_then(Value::as_str).unwrap_or("");
        if !is_group(raw_kind) {
            continue;
        }
        let now = item
            .get("now")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let nodes = item
            .get("all")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|node_name| {
                        let node = proxies.get(node_name);
                        let kind = node
                            .and_then(|item| item.get("type"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        ProxyNode {
                            name: node_name.to_string(),
                            label: display_name(node_name),
                            kind: pretty_kind(kind),
                            delay_ms: node.and_then(|item| member_delay(proxies, item, 0)),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let group = ProxyGroup {
            name: name.clone(),
            label: display_name(name),
            kind: pretty_kind(raw_kind),
            now: now.clone(),
            now_label: display_name(&now),
            nodes,
        };
        if name == "GLOBAL" {
            global = Some(group);
        } else {
            groups.push(group);
        }
    }
    if let Some(global) = global {
        groups.push(global);
    }
    groups
}

/// The group the tray and the overview act on. Global mode: GLOBAL. Otherwise the top-level select
/// group — one no other group contains, e.g. "🌍 出口" holding the fallback "🇺🇸 US" — then any
/// select group, then the first group. mihomo lists groups alphabetically, so "first group" alone
/// can land on an inner fallback such as "Ⓜ Meta".
pub fn main_group<'a>(groups: &'a [ProxyGroup], mode: &str) -> Option<&'a ProxyGroup> {
    if mode == "global" {
        return groups.iter().find(|g| g.name == "GLOBAL");
    }
    let rest = || groups.iter().filter(|g| g.name != "GLOBAL");
    let nested = |name: &str| rest().any(|g| g.nodes.iter().any(|n| n.name == name));
    rest()
        .find(|g| g.kind == "select" && !nested(&g.name))
        .or_else(|| rest().find(|g| g.kind == "select"))
        .or_else(|| rest().next())
        .or(groups.first())
}

pub fn connections_from(value: &Value, now: SystemTime) -> (Vec<Conn>, Totals) {
    let totals = Totals {
        down: size_text(number(value.get("downloadTotal"))),
        up: size_text(number(value.get("uploadTotal"))),
    };
    let Some(list) = value.get("connections").and_then(Value::as_array) else {
        return (Vec::new(), totals);
    };
    let mut rows = list
        .iter()
        .map(|item| {
            let down = number(item.get("download"));
            let up = number(item.get("upload"));
            let policy = item
                .get("chains")
                .and_then(Value::as_array)
                .and_then(|chains| chains.first())
                .and_then(Value::as_str)
                .unwrap_or("");
            (
                down,
                Conn {
                    id: text_at(item, "id"),
                    app: item
                        .get("metadata")
                        .map(|v| text_at(v, "process"))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "—".into()),
                    host: conn_host(item),
                    rule: text_at(item, "rule"),
                    policy: if policy.is_empty() {
                        "—".to_string()
                    } else {
                        policy_label(policy)
                    },
                    kind: policy_kind(policy),
                    down: size_text(down),
                    up: size_text(up),
                    age: age_text(item.get("start").and_then(Value::as_str).unwrap_or(""), now),
                },
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    (rows.into_iter().map(|(_, row)| row).collect(), totals)
}

pub fn rules_from(value: &Value) -> Vec<Rule> {
    let Some(list) = value.get("rules").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .enumerate()
        .map(|(index, item)| {
            let policy = item.get("proxy").and_then(Value::as_str).unwrap_or("");
            Rule {
                index: (index + 1).to_string(),
                kind: text_at(item, "type"),
                payload: text_at(item, "payload"),
                policy: policy_label(policy),
                tone: policy_kind(policy),
                hits: rule_hits(item),
            }
        })
        .collect()
}

/// PurrEarBandView.earOffset: straight segments between the ear points, with a cosine
/// dome across the crown (-15 … 15).
/// `extra.hitCount` of one `/rules` entry (mihomo ≥ 1.19.x); None when absent.
pub fn rule_hits(item: &Value) -> Option<u64> {
    item.get("extra")?.get("hitCount")?.as_u64()
}

/// The protocol a member really connects with: a nested group (fallback "🇺🇸 US" inside
/// "🌍 出口") is followed through `now` to the node it currently uses. None when the chain cannot
/// be resolved (unknown name or a loop).
#[cfg_attr(not(test), allow(dead_code))] // used by the macOS FFI (lib), not the Windows binary
pub fn leaf_kind(root: &Value, name: &str) -> Option<String> {
    let proxies = root.get("proxies")?.as_object()?;
    let mut current = name;
    for _ in 0..6 {
        let item = proxies.get(current)?;
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
        if !is_group(kind) {
            return Some(pretty_kind(kind));
        }
        current = item.get("now").and_then(Value::as_str)?;
    }
    None
}

pub fn ear_offset(dx: f32, k: f32) -> f32 {
    const PTS: [(f32, f32); 7] = [
        (-50.0, 0.0),
        (-33.0, -46.0),
        (-15.0, -19.0),
        (0.0, -22.0),
        (15.0, -19.0),
        (33.0, -46.0),
        (50.0, 0.0),
    ];
    if k == 0.0 {
        return 0.0;
    }
    let x = dx / k;
    if x <= -50.0 || x >= 50.0 {
        return 0.0;
    }
    if (-15.0..=15.0).contains(&x) {
        return (-19.0 - 3.2 * ((x / 15.0) * std::f32::consts::FRAC_PI_2).cos()) * k;
    }
    for i in 0..6 {
        let (a, ay) = PTS[i];
        let (b, by) = PTS[i + 1];
        if x >= a && x <= b {
            return (ay + (by - ay) * (x - a) / (b - a)) * k;
        }
    }
    0.0
}

#[allow(clippy::too_many_arguments)]
pub fn ear_wave_at(
    width: f32,
    height: f32,
    ear_x: f32,
    scale: f32,
    y_origin: f32,
    millis: f32,
    awake: bool,
    hold: f32,
    load: f32,
) -> Wave {
    if width < 8.0 || height < 8.0 {
        return Wave {
            main: "M 0 0".to_string(),
            warm: String::new(),
        };
    }
    let step = 3.0;
    let split = if awake {
        -1.0
    } else {
        width * hold.clamp(0.0, 1.0)
    };
    let mut main = Vec::new();
    let mut warm = Vec::new();
    let mut x = 0.0;
    while x <= width + 0.1 {
        let distance = (x - ear_x).abs() / width.max(1.0);
        let mut y = y_origin + ear_offset(x - ear_x, scale);
        if awake {
            let breath = 0.55 + 0.45 * (millis / 900.0).sin();
            let amp = (1.1 + load * 4.5) * breath * (1.0 - distance * 0.5);
            y += (x * 0.42 + millis / 18.0).sin() * amp;
            y += (x * 0.11 - millis / 60.0).sin() * amp * 0.6;
        } else if hold > 0.0 {
            y += (x * 0.42 + millis / 30.0).sin() * hold * 2.4;
        }
        if split > 0.0 && x <= split + 0.1 {
            warm.push((x, y));
        }
        if split <= 0.0 || x >= split - 0.1 {
            main.push((x, y));
        }
        x += step;
    }
    Wave {
        main: svg_polyline(&main, None),
        warm: if warm.len() > 1 {
            svg_polyline(&warm, None)
        } else {
            "M 0 0".to_string()
        },
    }
}

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn ear_wave(
    width: f32,
    height: f32,
    scale: f32,
    y_ratio: f32,
    millis: f32,
    awake: bool,
    hold: f32,
    load: f32,
) -> Wave {
    ear_wave_at(
        width,
        height,
        width / 2.0,
        scale,
        height * y_ratio,
        millis,
        awake,
        hold,
        load,
    )
}

/// Logarithmic latency axis used by PURR desktop v5 (15–250 ms).
pub fn flood_path(width: f32, height: f32, origin: f32, progress: f32) -> String {
    // PurrMainView: maxR = hypot(w, h) * 1.15, centred at (floodX, 110).
    let radius = width.hypot(height) * 1.15 * progress.clamp(0.0, 1.0);
    let mut left = Vec::new();
    let mut right = Vec::new();
    for y in 0..=height.ceil() as usize {
        let y = (y as f32).min(height);
        let square = radius * radius - (y - 110.0).powi(2);
        if square < 0.0 {
            continue;
        }
        let span = square.sqrt();
        let (a, b) = ((origin - span).max(0.0), (origin + span).min(width));
        if a <= b {
            left.push((a, y));
            right.push((b, y));
        }
    }
    left.extend(right.into_iter().rev());
    if left.is_empty() {
        "M0 0".into()
    } else {
        format!("{} Z", svg_polyline(&left, None))
    }
}

pub fn spectrum_position(delay: f32) -> f32 {
    let d = delay.clamp(15.0, 245.0);
    let t_lin = (d - 15.0) / (250.0 - 15.0);
    let t_log = (d / 15.0).ln() / (250.0f32 / 15.0).ln();
    (t_lin * 0.62 + t_log * 0.38).clamp(0.02, 0.96)
}

/// Port of PurrEarBandView's canvas: 2 px polyline, mode-specific motion and the
/// spring stretch. `speed` is PurrStore.currentEarX's normalised speed (0 … 1).
#[allow(clippy::too_many_arguments)]
pub fn ear_wave_v5(
    width: f32,
    height: f32,
    ear_x: f32,
    speed: f32,
    millis: f32,
    awake: bool,
    hold: f32,
    load: f32,
    mode: &str,
    trouble: bool,
    reduced: bool,
    _twitch: f32,
    dashed: bool,
) -> Wave {
    if width < 8.0 || height < 8.0 {
        return Wave {
            main: "M0 0".into(),
            warm: "M0 0".into(),
        };
    }
    let time = if reduced { 0.0 } else { millis };
    let speed = speed.clamp(0.0, 1.0);
    let stretch = 1.0 + speed * 0.6;
    let hop = (1.0 - speed * 0.55) * if trouble { 0.5 } else { 1.0 };
    let max_x = width as i32;
    let y_at = |xi: i32| -> f32 {
        let x = xi as f32;
        let dx = (x - ear_x) / stretch;
        let mut y = 76.0 + ear_offset(dx, 1.0) * hop;
        if awake && trouble {
            let d = (x - ear_x).abs();
            if d > 46.0 {
                let f = (1.0 - (d - 46.0) / 320.0).max(0.0);
                let zig = 1.0 - ((x % 8.0) - 4.0).abs() / 4.0;
                y -= f * zig * (6.0 + 2.5 * (time / 60.0 + x * 0.7).sin());
            }
        } else if awake {
            let breath = 0.55 + 0.45 * (time / 900.0).sin();
            let distance = (x - ear_x).abs() / width;
            let mut amp = (1.1 + load * 4.5) * breath * (1.0 - distance * 0.5);
            match mode {
                "global" => amp = amp * 1.5 + 1.4,
                "direct" => amp = 0.0,
                _ => {
                    let near = (1.0 - (x - ear_x).abs() / 80.0).max(0.0);
                    let segment = ((((x - time * 0.06) / 34.0).sin() + 0.15) * 2.4).clamp(0.0, 1.0);
                    amp *= near.max(segment);
                }
            }
            y += (x * 0.42 + time / 18.0).sin() * amp + (x * 0.11 - time / 60.0).sin() * amp * 0.6;
            if mode == "direct" {
                y += (x * 0.018 - time / 800.0).sin() * 2.0;
            }
        } else if hold > 0.0 && !reduced {
            y += (x * 0.42 + time / 30.0).sin() * hold * 2.4;
        }
        y
    };
    // buildSubpath(from:to:): f, f+2, … (< limit), then limit.
    let sub = |f: i32, end: i32| -> Vec<(f32, f32)> {
        let mut pts = Vec::new();
        if f > end || f > max_x {
            return pts;
        }
        let limit = end.min(max_x);
        pts.push((f as f32, y_at(f)));
        let mut i = f + 2;
        while i < limit {
            pts.push((i as f32, y_at(i)));
            i += 2;
        }
        pts.push((limit as f32, y_at(limit)));
        pts
    };
    // Hold progress splits the line: warm (waking) or 35 % ink (falling asleep) up to the split.
    let split = if hold > 0.0 {
        (width * hold.clamp(0.0, 1.0)).round() as i32
    } else {
        -1
    };
    if split >= 0 {
        let warm = sub(0, split);
        let main = sub(split, max_x);
        Wave {
            main: wave_path(&main, dashed),
            warm: if warm.len() > 1 {
                wave_path(&warm, false)
            } else {
                "M0 0".into()
            },
        }
    } else {
        Wave {
            main: wave_path(&sub(0, max_x), dashed),
            warm: "M0 0".into(),
        }
    }
}

fn wave_path(points: &[(f32, f32)], dashed: bool) -> String {
    use std::fmt::Write;
    let Some(&(x, y)) = points.first() else {
        return "M0 0".into();
    };
    let mut out = String::with_capacity(points.len() * 24);
    write!(out, "M{x:.3} {y:.3}").unwrap();
    let mut remaining = 5.0f32;
    let mut draw = true;
    for pair in points.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        if !dashed {
            write!(out, " L{bx:.3} {by:.3}").unwrap();
            continue;
        }
        let length = (bx - ax).hypot(by - ay);
        let mut travelled = 0.0;
        while length - travelled > 0.00001 {
            let step = remaining.min(length - travelled);
            travelled += step;
            remaining -= step;
            let fraction = travelled / length;
            let (x, y) = (ax + (bx - ax) * fraction, ay + (by - ay) * fraction);
            if draw {
                write!(out, " L{x:.3} {y:.3}").unwrap();
            }
            if remaining < 0.00001 {
                draw = !draw;
                remaining = if draw { 5.0 } else { 8.0 };
                if draw {
                    write!(out, " M{x:.3} {y:.3}").unwrap();
                }
            }
        }
    }
    out
}
/// Samples the overview chart and its "60 秒峰值" cover: one per second, so exactly the last 60 s.
pub const HISTORY_SAMPLES: usize = 60;

/// Appends one per-second sample and keeps only the last `HISTORY_SAMPLES`.
pub fn push_history(hist: &mut Vec<(u64, u64)>, sample: (u64, u64)) {
    hist.push(sample);
    if hist.len() > HISTORY_SAMPLES {
        let extra = hist.len() - HISTORY_SAMPLES;
        hist.drain(0..extra);
    }
}

/// Highest download rate in the last `HISTORY_SAMPLES` samples; None when there are none.
pub fn peak_down(hist: &[(u64, u64)]) -> Option<u64> {
    hist[hist.len().saturating_sub(HISTORY_SAMPLES)..]
        .iter()
        .map(|(_, d)| *d)
        .max()
}

pub fn peak_up(hist: &[(u64, u64)]) -> Option<u64> {
    hist[hist.len().saturating_sub(HISTORY_SAMPLES)..]
        .iter()
        .map(|(u, _)| *u)
        .max()
}

/// The traffic chart: the data lines are drawn exactly at their values (zero traffic is a flat line
/// on the zero grid line); nothing decorative is added to them.
pub fn chart(hist: &[(u64, u64)], width: f32, height: f32) -> Chart {
    let w = (width - 58.0).max(1.0);
    let top = 8.0;
    let bottom = (height - 20.0).max(top + 1.0);
    let peak = peak_down(hist).unwrap_or(0) as f32 / MB as f32;
    let max = (peak / 5.0).ceil().max(1.0) * 5.0;
    let y = |v: f32| bottom - v / max * (bottom - top);
    let n = HISTORY_SAMPLES;
    let tail = &hist[hist.len().saturating_sub(n)..];
    let mut samples = vec![(0u64, 0u64); n.saturating_sub(tail.len())];
    samples.extend_from_slice(tail);
    let points = |up: bool| {
        samples
            .iter()
            .enumerate()
            .map(|(i, (u, d))| {
                let value = if up { *u } else { *d } as f32 / MB as f32;
                (i as f32 / (n - 1) as f32 * w, y(value).clamp(top, bottom))
            })
            .collect::<Vec<_>>()
    };
    let down = points(false);
    let up = points(true);
    // Four-pixel dashes, measured along the polyline (Slint Path has no dash array).
    let mut dashed = String::new();
    let mut distance = 0.0f32;
    for pair in up.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        let mut at = 0.0;
        while at < length {
            let remaining = 4.0 - distance % 4.0;
            let end = (at + remaining.max(0.001)).min(length);
            if (distance / 4.0).floor() as i32 % 2 == 0 {
                let point = |d: f32| {
                    (
                        a.0 + (b.0 - a.0) * d / length,
                        a.1 + (b.1 - a.1) * d / length,
                    )
                };
                let (p, q) = (point(at), point(end));
                dashed.push_str(&format!("M{:.1} {:.1} L{:.1} {:.1} ", p.0, p.1, q.0, q.1));
            }
            distance += end - at;
            at = end;
        }
    }
    Chart {
        fill: svg_polyline(&down, Some(bottom)),
        down: svg_polyline(&down, None),
        up: dashed,
        grid: [0.0, max / 2.0, max]
            .iter()
            .map(|v| format!("M0 {:.1} H{w:.1} ", y(*v)))
            .collect(),
        peak: rate_text(peak_down(hist).unwrap_or(0)),
    }
}

/// Members a speed test probes: real nodes and nested groups (mihomo measures a group through the
/// node it currently uses, e.g. a fallback "🇺🇸 US" inside "🌍 出口"); never DIRECT/REJECT/relay.
pub fn probe_delay(kind: &str) -> bool {
    !matches!(
        kind,
        "direct" | "reject" | "compatible" | "pass" | "relay" | "—"
    )
}

pub fn skip_delay(kind: &str) -> bool {
    matches!(
        kind,
        "select"
            | "url-test"
            | "fallback"
            | "load-balance"
            | "direct"
            | "reject"
            | "compatible"
            | "pass"
            | "relay"
            | "—"
    )
}

fn svg_polyline(points: &[(f32, f32)], close_y: Option<f32>) -> String {
    let Some((first_x, first_y)) = points.first().copied() else {
        return "M 0 0".to_string();
    };
    let mut out = format!("M {first_x:.1} {first_y:.1}");
    for (x, y) in points.iter().skip(1) {
        out.push_str(&format!(" L {x:.1} {y:.1}"));
    }
    if let Some(y) = close_y {
        let last_x = points.last().map(|(x, _)| *x).unwrap_or(first_x);
        out.push_str(&format!(" L {last_x:.1} {y:.1} L {first_x:.1} {y:.1} Z"));
    }
    out
}

fn is_emoji(ch: char) -> bool {
    let code = ch as u32;
    (0x1F1E6..=0x1F1FF).contains(&code)
        || (0x1F300..=0x1FAFF).contains(&code)
        || (0x2460..=0x24FF).contains(&code)
        || (0x2600..=0x27BF).contains(&code)
        || ch == '\u{FE0F}'
        || ch == '\u{200D}'
}

fn is_group(kind: &str) -> bool {
    matches!(kind, "Selector" | "URLTest" | "Fallback" | "LoadBalance")
}

fn pretty_kind(kind: &str) -> String {
    match kind {
        "URLTest" => "url-test".to_string(),
        "LoadBalance" => "load-balance".to_string(),
        "Selector" => "select".to_string(),
        "Fallback" => "fallback".to_string(),
        "Shadowsocks" => "ss".to_string(),
        "Vmess" | "VMess" => "vmess".to_string(),
        "Direct" => "direct".to_string(),
        "Reject" => "reject".to_string(),
        "Compatible" | "Pass" | "Relay" => kind.to_ascii_lowercase(),
        "" => "—".to_string(),
        other => other.to_string(),
    }
}

/// A group member's delay: its own last probe, or — when the member is itself a group (fallback /
/// url-test / select) with no history of its own — the delay of the node that group currently uses.
fn member_delay(proxies: &serde_json::Map<String, Value>, node: &Value, depth: u8) -> Option<u32> {
    if let Some(d) = history_delay(node) {
        return Some(d);
    }
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    if depth >= 4 || !is_group(kind) {
        return None;
    }
    let now = node.get("now").and_then(Value::as_str)?;
    member_delay(proxies, proxies.get(now)?, depth + 1)
}

fn history_delay(node: &Value) -> Option<u32> {
    let delay = node
        .get("history")?
        .as_array()?
        .last()?
        .get("delay")?
        .as_u64()?;
    if delay == 0 {
        None
    } else {
        u32::try_from(delay).ok()
    }
}

fn policy_kind(name: &str) -> i32 {
    match name {
        "DIRECT" => 1,
        "REJECT" | "REJECT-DROP" => 2,
        _ => 0,
    }
}

fn policy_label(name: &str) -> String {
    match name {
        "DIRECT" => "直连".to_string(),
        "REJECT" | "REJECT-DROP" => "拒绝".to_string(),
        "" => "—".to_string(),
        other => display_name(other),
    }
}

fn conn_host(item: &Value) -> String {
    let meta = item.get("metadata");
    let host = meta
        .and_then(|item| item.get("host"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !host.is_empty() {
        return host.to_string();
    }
    let ip = meta
        .and_then(|item| item.get("destinationIP"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let port = match meta.and_then(|item| item.get("destinationPort")) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    };
    if !ip.is_empty() {
        if port.is_empty() {
            return ip.to_string();
        }
        return format!("{ip}:{port}");
    }
    let payload = text_at(item, "rulePayload");
    if payload.is_empty() {
        "—".to_string()
    } else {
        payload
    }
}

fn text_at(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn first_text(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }
    String::new()
}

fn number(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0)
}

/// Seconds since a connection's RFC 3339 `start` (offset honoured); None when it does not parse.
pub fn age_secs(start: &str, now: SystemTime) -> Option<u64> {
    let start = parse_rfc3339(start)?;
    Some(now.duration_since(start).map(|d| d.as_secs()).unwrap_or(0))
}

pub fn age_text(start: &str, now: SystemTime) -> String {
    let Some(seconds) = age_secs(start, now) else {
        return "—".to_string();
    };
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

pub fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    if text.len() < 19 {
        return None;
    }
    let year: i32 = text[0..4].parse().ok()?;
    let month: u32 = text[5..7].parse().ok()?;
    let day: u32 = text[8..10].parse().ok()?;
    let hour: u32 = text[11..13].parse().ok()?;
    let minute: u32 = text[14..16].parse().ok()?;
    let second: u32 = text[17..19].parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    let mut unix =
        days * 86400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    let mut rest = &text[19..];
    if let Some(stripped) = rest.strip_prefix('.') {
        rest = stripped.trim_start_matches(|ch: char| ch.is_ascii_digit());
    }
    let (sign, digits) = if let Some(rest) = rest.strip_prefix('+') {
        (1, rest)
    } else if let Some(rest) = rest.strip_prefix('-') {
        (-1, rest)
    } else {
        (0, "")
    };
    if sign != 0 && digits.len() >= 5 {
        let hours: i64 = digits[0..2].parse().ok()?;
        let minutes: i64 = digits[3..5].parse().ok()?;
        unix -= sign * (hours * 3600 + minutes * 60);
    }
    if unix < 0 {
        return None;
    }
    UNIX_EPOCH.checked_add(Duration::from_secs(unix as u64))
}

fn days_from_civil(mut year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    if month <= 2 {
        year -= 1;
    }
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = (year - era * 400) as u64;
    let day_of_year =
        (153 * (month as i32 + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year as u64;
    Some(i64::from(era) * 146097 + day_of_era as i64 - 719468)
}

/// DenseAsciiCatView (PurrIcons.swift): a 48 × 22 character cat, re-sampled every frame with
/// `alt = (t / 0.45) is odd` and `tail = sin(t * 2.8) * 0.06`. Returns one string per
/// (row, alpha) layer so the UI can draw it with a handful of text runs.
pub fn ascii_cat(kind: &str, alt: bool, tail: f64) -> Vec<(i32, f32, String)> {
    const COLS: usize = 48;
    const ROWS: usize = 22;
    const RAMP: &[u8] = b" .,:;-=+*#%@";
    fn dist_seg(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
        let (dx, dy) = (x2 - x1, y2 - y1);
        if dx == 0.0 && dy == 0.0 {
            return (px - x1).hypot(py - y1);
        }
        let t = (((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        (px - (x1 + t * dx)).hypot(py - (y1 + t * dy))
    }
    fn in_poly(x: f64, y: f64, poly: &[(f64, f64)]) -> bool {
        let mut inside = false;
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi) {
                inside = !inside;
            }
            j = i;
        }
        inside
    }
    fn tail_dist(x: f64, y: f64, pts: &[(f64, f64)]) -> f64 {
        let mut d = 99.0f64;
        for w in pts.windows(2) {
            d = d.min(dist_seg(x, y, w[0].0, w[0].1, w[1].0, w[1].1));
        }
        d
    }
    fn shade(x: f64, y: f64) -> (char, f64) {
        let nx = (x + 0.15) / 0.75;
        let ny = (y - 0.10) / 0.65;
        let lum = (0.56 - 0.42 * nx - 0.36 * ny).clamp(0.0, 1.0);
        let idx = (((1.0 - lum * 0.78) * 11.0).round() as i64).clamp(1, 11) as usize;
        (RAMP[idx] as char, 0.58 + 0.42 * (idx as f64 / 11.0))
    }
    let sample = |x: f64, y: f64| -> Option<(char, f64)> {
        match kind {
            "empty" | "timeout" => {
                let d_head = ((x + 0.42) / 0.38).hypot((y + 0.06) / 0.35);
                let in_ear = in_poly(x, y, &[(-0.76, -0.22), (-0.68, -0.78), (-0.44, -0.34)])
                    || in_poly(x, y, &[(-0.40, -0.34), (-0.16, -0.78), (-0.08, -0.22)]);
                let d_body = ((x - 0.18) / 0.62).hypot((y - 0.22) / 0.44);
                let in_body = d_body <= 1.0 && y <= 0.62;
                let d_paws = ((x + 0.42) / 0.34).hypot((y - 0.46) / 0.18);
                let in_paws = d_paws <= 1.0 && y <= 0.62;
                let in_tail = tail_dist(
                    x,
                    y,
                    &[
                        (0.74, 0.35),
                        (0.95, 0.48 + tail),
                        (0.85, 0.64),
                        (0.45, 0.66),
                    ],
                ) < 0.085;
                if kind == "timeout" {
                    if (x - 0.10).hypot(y + 0.55) < 0.045 {
                        return Some(('.', 0.65));
                    }
                    if (x - 0.28).hypot(y + 0.65) < 0.055 {
                        return Some(('o', if alt { 0.85 } else { 0.40 }));
                    }
                    if (x - 0.48).hypot(y + 0.75) < 0.065 {
                        return Some(('O', if alt { 0.40 } else { 0.95 }));
                    }
                }
                if !(d_head <= 1.0 || in_ear || in_body || in_paws || in_tail) {
                    return None;
                }
                if ((x + 0.42) / 0.28).hypot((y + 0.02) / 0.22) < 1.0 {
                    return Some(('.', 0.22));
                }
                if (d_head - 0.96).abs() < 0.08 && x > -0.18 && y < 0.25 {
                    return None;
                }
                Some(shade(x, y))
            }
            "offline" | "sub" => {
                let d_head = ((x + 0.20) / 0.40).hypot((y + 0.30) / 0.34);
                #[allow(clippy::type_complexity)]
                let (el, er): (&[(f64, f64)], &[(f64, f64)]) = if kind == "sub" {
                    (
                        &[(-0.56, -0.38), (-0.66, -0.76), (-0.28, -0.56)],
                        &[(-0.12, -0.56), (0.26, -0.76), (0.16, -0.38)],
                    )
                } else {
                    (
                        &[(-0.54, -0.44), (-0.48, -0.92), (-0.24, -0.56)],
                        &[(-0.16, -0.56), (0.08, -0.92), (0.14, -0.44)],
                    )
                };
                let in_ear = in_poly(x, y, el) || in_poly(x, y, er);
                let by = (y - 0.32) / 0.56;
                let bw = 0.32 + 0.14 * ((y + 0.05) / 0.85).clamp(0.0, 1.0);
                let d_body = ((x + 0.12) / bw).hypot(by);
                let d_paw1 = ((x + 0.34) / 0.13).hypot((y - 0.83) / 0.10);
                let d_paw2 = ((x + 0.08) / 0.13).hypot((y - 0.83) / 0.10);
                let in_tail = tail_dist(
                    x,
                    y,
                    &[
                        (0.20, 0.76),
                        (0.48, 0.72),
                        (0.66 + tail, 0.45),
                        (0.68 + tail, 0.08),
                        (0.58 - tail, -0.22),
                        (0.72 - tail, -0.38),
                    ],
                ) < 0.095;
                if kind == "offline" {
                    if (x + 0.72).hypot(y + 0.65) < 0.06 {
                        return Some(('?', if alt { 0.95 } else { 0.45 }));
                    }
                    if (x - 0.32).hypot(y + 0.68) < 0.06 {
                        return Some(('?', if alt { 0.45 } else { 0.95 }));
                    }
                } else if y > 0.68 && y < 0.88 && x > -0.95 && x < -0.52 {
                    return Some(if y < 0.74 { ('_', 0.75) } else { ('=', 0.85) });
                }
                if !(d_head <= 1.0
                    || in_ear
                    || (d_body <= 1.0 && y > -0.15)
                    || d_paw1 <= 1.0
                    || d_paw2 <= 1.0
                    || in_tail)
                {
                    return None;
                }
                if ((x + 0.20) / 0.29).hypot((y + 0.26) / 0.21) < 1.0 {
                    return Some(('.', 0.22));
                }
                Some(shade(x, y))
            }
            _ => {
                let d_head = ((x + 0.52) / 0.34).hypot((y + 0.08) / 0.32);
                let in_ear = in_poly(x, y, &[(-0.80, -0.22), (-0.75, -0.72), (-0.55, -0.32)])
                    || in_poly(x, y, &[(-0.48, -0.32), (-0.28, -0.72), (-0.23, -0.22)]);
                let in_spine = tail_dist(
                    x,
                    y,
                    &[
                        (-0.35, 0.05),
                        (-0.12, -0.35),
                        (0.12, -0.45),
                        (0.35, -0.30),
                        (0.52, 0.08),
                    ],
                ) < 0.24;
                let leg =
                    |x1: f64, y1: f64, x2: f64, y2: f64| dist_seg(x, y, x1, y1, x2, y2) < 0.085;
                let legs = leg(-0.42, 0.15, -0.46, 0.78)
                    || leg(-0.25, 0.15, -0.26, 0.78)
                    || leg(0.35, 0.15, 0.34, 0.78)
                    || leg(0.52, 0.12, 0.55, 0.78);
                let in_tail = tail_dist(
                    x,
                    y,
                    &[
                        (0.54, 0.02),
                        (0.72 + tail, -0.35),
                        (0.68 - tail, -0.78),
                        (0.78, -0.88),
                    ],
                ) < 0.12;
                if (x + 0.10).hypot(y + 0.75) < 0.06 {
                    return Some(('!', if alt { 1.0 } else { 0.5 }));
                }
                if !(d_head <= 1.0 || in_ear || in_spine || legs || in_tail) {
                    return None;
                }
                if ((x + 0.52) / 0.25).hypot((y + 0.05) / 0.20) < 1.0 {
                    return Some(('.', 0.22));
                }
                Some(shade(x, y))
            }
        }
    };
    let stamps: (usize, usize, Vec<String>) = match kind {
        "empty" => (
            9,
            9,
            vec![
                "  ===   ===  ".into(),
                "=           =".into(),
                "    _w_      ".into(),
            ],
        ),
        "timeout" => (
            9,
            9,
            vec![
                "  \\       /  ".into(),
                "=  >  .  <  =".into(),
                "  /  _w_  \\  ".into(),
            ],
        ),
        "offline" => (
            6,
            13,
            vec![
                "  .-.   .-.  ".into(),
                if alt {
                    "= (.@) (.@) ="
                } else {
                    "= (@.) (@.) ="
                }
                .into(),
                "   ` _w_ `   ".into(),
            ],
        ),
        "sub" => (
            6,
            13,
            vec![
                "  ___   ___  ".into(),
                if alt {
                    "=  T,   ,T  ="
                } else {
                    "=  T;   ;T  ="
                }
                .into(),
                "    ,---,    ".into(),
            ],
        ),
        _ => (
            9,
            8,
            vec![
                " \\       / ".into(),
                "=(@) . (@)=".into(),
                "  /^^^^^\\  ".into(),
            ],
        ),
    };
    let mut face = std::collections::HashMap::new();
    for (dr, line) in stamps.2.iter().enumerate() {
        for (dc, ch) in line.chars().enumerate() {
            if ch != ' ' {
                face.insert((stamps.0 + dr) * COLS + stamps.1 + dc, ch);
            }
        }
    }
    let mut layers: Vec<(i32, f32, Vec<char>)> = Vec::new();
    for r in 0..ROWS {
        let y = (r as f64 + 0.5) / ROWS as f64 * 2.0 - 1.0;
        for c in 0..COLS {
            let cell = if let Some(ch) = face.get(&(r * COLS + c)) {
                Some((*ch, 1.0))
            } else {
                let x = (c as f64 + 0.5) / COLS as f64 * 2.4 - 1.2;
                sample(x, y)
            };
            let Some((ch, alpha)) = cell else { continue };
            let a = (alpha * 100.0).round() as f32 / 100.0;
            let slot = match layers
                .iter()
                .position(|l| l.0 == r as i32 && (l.1 - a).abs() < 0.001)
            {
                Some(i) => i,
                None => {
                    layers.push((r as i32, a, vec![' '; COLS]));
                    layers.len() - 1
                }
            };
            layers[slot].2[c] = ch;
        }
    }
    layers
        .into_iter()
        .map(|(r, a, chars)| {
            (
                r,
                a,
                chars.into_iter().collect::<String>().trim_end().to_string(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn display_name_drops_flag_emoji() {
        assert_eq!(display_name("🇺🇸 US"), "US");
        assert_eq!(display_name("🌍 出口"), "出口");
        assert_eq!(display_name("Ⓜ Meta"), "Meta");
        assert_eq!(display_name("TR-Reality"), "TR-Reality");
    }

    #[test]
    fn ear_is_highest_at_the_tips() {
        assert!(ear_offset(-33.0, 1.0) < -40.0);
        assert!(ear_offset(0.0, 1.0) < -20.0);
        assert_eq!(ear_offset(80.0, 1.0), 0.0);
    }

    #[test]
    fn rates_and_sizes_use_decimal_units() {
        assert_eq!(
            rate_parts(5_000_000),
            ("5.0".to_string(), "MB/s".to_string())
        );
        assert_eq!(rate_parts(1500), ("2".to_string(), "KB/s".to_string()));
        assert_eq!(rate_parts(0), ("0".to_string(), "KB/s".to_string()));
        assert_eq!(rate_parts(30_000), ("30".to_string(), "KB/s".to_string()));
        assert_eq!(rate_parts(999_999).1, "KB/s");
        assert_eq!(rate_text(1_000_000), "1.0 MB/s");
        // A provider quota of 883 GB used (as the provider prints it) is shown as 883, not 822.
        assert_eq!(gb_text(883_000_000_000), "883");
        assert_eq!(gb_text(400_000_000), "0.4");
        assert_eq!(size_text(1_500_000_000), "1.50 GB");
        assert_eq!(mb_text(95_000_000), "95.0");
        assert!(is_reject_log(
            "[TCP] 127.0.0.1:56829 --> stats.g.doubleclick.net:443 match DomainSuffix(doubleclick.net) using REJECT"
        ));
        assert!(is_reject_log(
            "[TCP] 198.18.0.1:5 --> x.example:443 match GeoSite(category-ads-all) using REJECT-DROP"
        ));
        assert!(!is_reject_log(
            "[UDP] 198.18.0.1:57581(DoubaoIme) --> 106.38.222.182:443 match AND(((Network,udp))) using REJECT"
        ));
        assert!(!is_reject_log(
            "[TCP] 127.0.0.1:1 --> github.com:443 match Match using 🌍 出口[东京]"
        ));
        assert_eq!(size_text(1000), "1 KB");
    }

    #[test]
    fn place_json_skips_rate_limits_and_reads_fallbacks() {
        assert_eq!(
            place_from_json(&json!({"status": 429, "error": {"title": "Rate limit"}})),
            None
        );
        assert_eq!(
            place_from_json(&json!({"ip": "1.2.3.4", "city": "Tokyo", "country": "JP"})),
            Some("1.2.3.4 · Tokyo, JP".to_string())
        );
        assert_eq!(
            place_from_json(
                &json!({"success": true, "ip": "1.2.3.4", "city": "Osaka", "country_code": "JP"})
            ),
            Some("1.2.3.4 · Osaka, JP".to_string())
        );
        assert_eq!(
            place_from_json(
                &json!({"status": "success", "query": "8.8.8.8", "city": "Seoul", "countryCode": "KR"})
            ),
            Some("8.8.8.8 · Seoul, KR".to_string())
        );
        assert_eq!(place_from_json(&json!({"status": "fail"})), None);
    }

    #[test]
    fn groups_keep_selectors_and_global_last() {
        let value = json!({
            "proxies": {
                "GLOBAL": {"type": "Selector", "now": "DIRECT", "all": ["DIRECT", "REJECT"]},
                "出口": {"type": "Selector", "now": "🇺🇸 US", "all": ["🇺🇸 US", "TR-Reality"]},
                "自动": {"type": "URLTest", "now": "🇺🇸 US", "all": ["🇺🇸 US"]},
                "🇺🇸 US": {"type": "VLESS", "history": [{"delay": 32}]},
                "TR-Reality": {"type": "VLESS"}
            }
        });
        let groups = proxy_groups(&value);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "出口");
        assert_eq!(groups[0].now_label, "US");
        assert_eq!(groups[0].nodes[0].label, "US");
        assert_eq!(groups[0].nodes[0].delay_ms, Some(32));
        assert_eq!(groups[1].kind, "url-test");
        assert_eq!(groups[2].name, "GLOBAL");
    }

    #[test]
    fn connections_and_rules_parse() {
        let now = UNIX_EPOCH + Duration::from_secs(90);
        let (rows, totals) = connections_from(
            &json!({
                "downloadTotal": 2048,
                "uploadTotal": 512,
                "connections": [{
                    "metadata": {"host": "example.com", "destinationPort": "443"},
                    "download": 100,
                    "upload": 10,
                    "start": "1970-01-01T00:00:00Z",
                    "chains": ["🇺🇸 US"],
                    "rule": "DomainSuffix"
                }]
            }),
            now,
        );
        assert_eq!(totals.down, "2 KB");
        assert_eq!(rows[0].host, "example.com");
        assert_eq!(rows[0].policy, "US");
        assert_eq!(rows[0].age, "1m30s");
        let rules = rules_from(&json!({
            "rules": [
                {"type": "DomainSuffix", "payload": "google.com", "proxy": "出口"},
                {"type": "GeoIP", "payload": "CN", "proxy": "DIRECT"},
                {"type": "GeoSite", "payload": "ads", "proxy": "REJECT"}
            ]
        }));
        assert_eq!(rules[1].policy, "直连");
        assert_eq!(rules[1].tone, 1);
        assert_eq!(rules[2].policy, "拒绝");
        assert_eq!(rules[2].tone, 2);
    }

    #[test]
    fn rfc3339_offsets_collapse_to_epoch() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(UNIX_EPOCH));
        assert_eq!(parse_rfc3339("1970-01-01T08:00:00+08:00"), Some(UNIX_EPOCH));
    }

    #[test]
    fn connection_age_honours_the_offset() {
        // mihomo writes local time with an offset; 17:01:12+08:00 is 09:01:12 UTC.
        let start = "2026-11-08T17:01:12.27+08:00";
        let then = parse_rfc3339(start).unwrap();
        let now = then + Duration::from_secs(125);
        assert_eq!(age_secs(start, now), Some(125));
        assert_eq!(age_text(start, now), "2m05s");
        assert_eq!(
            parse_rfc3339("2026-11-08T09:01:12Z"),
            parse_rfc3339("2026-11-08T17:01:12+08:00")
        );
        assert_eq!(age_secs("garbage", now), None);
        assert_eq!(age_text("", now), "—");
    }

    #[test]
    fn chart_covers_sixty_seconds_and_zero_is_flat() {
        let mut hist = Vec::new();
        for i in 0..150u64 {
            push_history(&mut hist, (i, i * 1000));
        }
        assert_eq!(hist.len(), HISTORY_SAMPLES);
        assert_eq!(hist[0], (90, 90_000));
        assert_eq!(peak_down(&hist), Some(149_000));
        assert_eq!(peak_down(&[]), None);
        let zeros = vec![(0u64, 0u64); HISTORY_SAMPLES];
        let plot = chart(&zeros, 740.0, 170.0);
        // Every point of both lines sits on the zero line (y = height - 20).
        for token in plot.down.split(['M', 'L']).filter(|t| !t.trim().is_empty()) {
            let y: f32 = token.split_whitespace().nth(1).unwrap().parse().unwrap();
            assert!((y - 150.0).abs() < 0.01, "{token}");
        }
        assert_eq!(plot.peak, "0 KB/s");
    }

    #[test]
    fn nested_group_shows_the_protocol_it_really_uses() {
        let root = json!({"proxies": {
            "🌍 出口": {"type": "Selector", "now": "🇺🇸 US"},
            "🇺🇸 US": {"type": "Fallback", "now": "US-Hy2"},
            "US-Hy2": {"type": "Hysteria2"}
        }});
        assert_eq!(leaf_kind(&root, "🇺🇸 US").as_deref(), Some("Hysteria2"));
        assert_eq!(leaf_kind(&root, "US-Hy2").as_deref(), Some("Hysteria2"));
        assert_eq!(leaf_kind(&root, "missing"), None);
        assert_eq!(rule_hits(&json!({"extra": {"hitCount": 15}})), Some(15));
        assert_eq!(rule_hits(&json!({"type": "Match"})), None);
    }
}
