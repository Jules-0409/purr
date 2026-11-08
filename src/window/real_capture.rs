// SPDX-License-Identifier: GPL-3.0-only
//! `Purr --preview --real-capture <out-dir> [--fixture <dir>]`: renders every page through the
//! real `apply` path (no `preview::decorate`, PREVIEW_MODE off), first with nothing measured yet
//! ("unavailable"), then fed through the same setters the live loops use with core API JSON saved
//! from a running core (`proxies.json`, `connections.json`, `rules.json`, optional
//! `preferences.json`, `statistics.json`, `traffic.json` = [[up, down], …] per second,
//! `proc.json` = {"shell": bytes, "core": bytes, "cpu": pct}, `place.txt`).
//! Never prepares config, starts a core, or changes system proxies.
use super::*;

fn read_json(dir: &std::path::Path, name: &str) -> Option<serde_json::Value> {
    std::fs::read(dir.join(name))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

fn feed(live: &Arc<Mutex<LiveState>>, dir: &std::path::Path) {
    if let Some(stats) = read_json(dir, "statistics.json") {
        if let Ok(t) = serde_json::from_value::<crate::telemetry::Telemetry>(stats) {
            live.lock().expect("live lock").telemetry = t;
        }
    }
    let mode = read_json(dir, "configs.json")
        .and_then(|v| v["mode"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "rule".into());
    set_awake(live, true);
    if let Some(proxies) = read_json(dir, "proxies.json") {
        set_directory(live, &mode, present::proxy_groups(&proxies));
        for g in present::proxy_groups(&proxies) {
            for n in g.nodes {
                if let Some(ms) = n.delay_ms {
                    set_delay(live, &n.name, format!("{ms}ms"));
                }
            }
        }
    }
    if let Some(conns) = read_json(dir, "connections.json") {
        set_connections(live, &conns);
    }
    if let Some(rules) = read_json(dir, "rules.json") {
        set_rules(live, &rules);
    }
    if let Some(traffic) = read_json(dir, "traffic.json") {
        for pair in traffic.as_array().into_iter().flatten() {
            let up = pair[0].as_u64().unwrap_or(0);
            let down = pair[1].as_u64().unwrap_or(0);
            {
                // One sample per second, as /traffic delivers them.
                live.lock().expect("live lock").last_sample = None;
            }
            set_speed(live, up, down);
        }
    }
    if let Some(proc_) = read_json(dir, "proc.json") {
        let mut g = live.lock().expect("live lock");
        g.memory_shell = proc_["shell"].as_u64().unwrap_or(0);
        g.memory_core = proc_["core"].as_u64().unwrap_or(0);
        g.cpu_pct = proc_["cpu"].as_f64();
        g.bump();
    }
    if let Ok(place) = std::fs::read_to_string(dir.join("place.txt")) {
        set_place(live, place.trim().to_string());
    }
    set_log_live(live);
}

pub fn run(out: &std::path::Path, fixture: Option<&std::path::Path>) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let live = Arc::new(Mutex::new(LiveState::default()));
    if let Some(prefs) = fixture.and_then(|d| read_json(d, "preferences.json")) {
        if let Ok(p) = serde_json::from_value::<crate::settings::Preferences>(prefs) {
            live.lock().expect("live lock").preferences = p;
        }
    }
    init(tx, live.clone(), "real-capture".into());
    open();
    with_ui(|ui| ui.set_reduced_motion(true));
    let mut frames: Vec<(String, i32, i32)> = Vec::new();
    for (page, pane, name) in [
        (0, 0, "overview"),
        (1, 0, "nodes"),
        (2, 0, "subscriptions"),
        (3, 0, "connections"),
        (4, 0, "rules"),
        (5, 0, "settings"),
    ] {
        frames.push((format!("win-real-unavailable-{name}"), page, pane));
    }
    let populated = fixture.is_some();
    if populated {
        for (page, pane, name) in [
            (0, 0, "overview"),
            (1, 0, "nodes"),
            (2, 0, "subscriptions"),
            (3, 0, "connections"),
            (4, 0, "rules"),
            (5, 0, "settings"),
            (0, -1, "tray"),
        ] {
            frames.push((format!("win-real-populated-{name}"), page, pane));
        }
    }
    let out = out.to_path_buf();
    let fixture = fixture.map(|p| p.to_path_buf());
    let timer = slint::Timer::default();
    let mut stage = 0usize;
    let mut phase = 0u8;
    let mut fed = false;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            if stage >= frames.len() {
                println!("real-capture: {} frames", frames.len());
                let _ = slint::quit_event_loop();
                return;
            }
            let (name, page, pane) = frames[stage].clone();
            if name.contains("populated") && !fed {
                if let Some(dir) = &fixture {
                    feed(&live, dir);
                }
                fed = true;
            }
            sync(&live);
            match phase {
                0 => {
                    with_ui(|ui| {
                        ui.set_page(page);
                        ui.set_conn_pane(pane.max(0));
                        ui.set_tray_open(pane < 0);
                    });
                    phase = 1;
                }
                1 => phase = 2,
                _ => {
                    with_ui(|ui| {
                        if let Err(e) = preview::capture(ui, &out.join(format!("{name}.ppm"))) {
                            eprintln!("real-capture {name}: {e}");
                        }
                    });
                    stage += 1;
                    phase = 0;
                }
            }
        },
    );
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    drop(timer);
    stop_motion();
    WINDOW.with(|slot| slot.borrow_mut().take());
    Ok(())
}
