// SPDX-License-Identifier: GPL-3.0-only
//! Isolated visual fixture runner. Never prepares config, starts a core, or changes system proxies.
use super::*;
use std::io::Write;

const NODES: [(&str, &str, u32); 11] = [
    ("东京 01", "Hysteria2", 32),
    ("东京 02", "Trojan", 38),
    ("大阪 01", "VLESS", 41),
    ("香港 03", "Hysteria2", 21),
    ("香港 05", "Trojan", 27),
    ("新加坡 02", "VLESS", 48),
    ("首尔 01", "Trojan", 58),
    ("台北 02", "VMess", 66),
    ("洛杉矶 01", "Hysteria2", 156),
    ("法兰克福 01", "Trojan", 188),
    ("伦敦 02", "VLESS", 212),
];
const JITTER: [i32; 11] = [3, 5, 6, 2, 4, 9, 7, 24, 11, 18, 14];
const LOSS: [i32; 11] = [0, 0, 1, 0, 0, 0, 2, 7, 0, 3, 1];
const UPTIME: [f32; 11] = [
    99.9, 99.7, 99.2, 99.9, 99.8, 99.5, 98.6, 94.1, 99.0, 97.3, 98.8,
];
thread_local! {
    static SYS: Cell<bool> = const { Cell::new(true) };
    static TICKS: Cell<u32> = const { Cell::new(0) };
    static TRAFFIC: Cell<(f64,f64)> = const { Cell::new((2090.0,1190.0)) };
    static LAST_AWAKE: Cell<u64> = const { Cell::new(0) };
    static HISTORY: RefCell<Vec<Vec<Option<f32>>>> = RefCell::new(histories());
}
fn histories() -> Vec<Vec<Option<f32>>> {
    let mut seed = 7u64;
    let mut random = || {
        seed = seed * 16807 % 2147483647;
        seed as f32 / 2147483647.0
    };
    NODES
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (0..18)
                .map(|_| {
                    if random() * 100.0 < LOSS[i] as f32 * 2.2 {
                        None
                    } else {
                        Some(
                            (n.2 as f32 + (random() + random() - 1.0) * JITTER[i] as f32 * 1.6)
                                .round()
                                .max(8.0),
                        )
                    }
                })
                .collect()
        })
        .collect()
}
fn state() -> LiveState {
    let groups = [
        ("节点选择", 0, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
        ("自动选择", 3, vec![0, 1, 3, 4, 5]),
        ("流媒体", 7, vec![3, 4, 7, 8]),
        ("AI 服务", 8, vec![6, 8, 9, 10]),
    ]
    .into_iter()
    .map(|(name, sel, ids)| ProxyGroup {
        name: name.into(),
        label: name.into(),
        kind: "select".into(),
        now: NODES[sel].0.into(),
        now_label: NODES[sel].0.into(),
        nodes: ids
            .into_iter()
            .map(|i| present::ProxyNode {
                name: NODES[i].0.into(),
                label: NODES[i].0.into(),
                kind: NODES[i].1.into(),
                delay_ms: Some(NODES[i].2),
            })
            .collect(),
    })
    .collect();
    let rules = [
        ("DOMAIN-SUFFIX", "openai.com", "AI 服务"),
        ("DOMAIN-SUFFIX", "anthropic.com", "AI 服务"),
        ("DOMAIN-SUFFIX", "claude.ai", "AI 服务"),
        ("DOMAIN-SUFFIX", "youtube.com", "流媒体"),
        ("DOMAIN-SUFFIX", "netflix.com", "流媒体"),
        ("DOMAIN-SUFFIX", "github.com", "节点选择"),
        ("DOMAIN-KEYWORD", "google", "节点选择"),
        ("DOMAIN-SUFFIX", "notion.so", "节点选择"),
        ("GEOSITE", "category-ads-all", "REJECT"),
        ("GEOSITE", "cn", "DIRECT"),
        ("GEOIP", "LAN", "DIRECT"),
        ("GEOIP", "CN", "DIRECT"),
        ("PROCESS-NAME", "Steam", "DIRECT"),
        ("MATCH", "*", "节点选择"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (kind, payload, policy))| Rule {
        index: (i + 1).to_string(),
        kind: kind.into(),
        payload: payload.into(),
        policy: policy.into(),
        tone: if policy == "DIRECT" {
            1
        } else if policy == "REJECT" {
            2
        } else {
            0
        },
    })
    .collect();
    let logs = [
        ("08:12:04", "purr", "摸醒了猫，连接到 东京 01"),
        ("11:40:19", "info", "抓到一只老鼠 → doubleclick.net"),
        (
            "14:05:33",
            "warn",
            "东京 01 有点凉（286ms），换窝到 香港 03",
        ),
        ("14:05:34", "info", "自动选择 → 香港 03"),
        ("17:58:02", "purr", "哄猫睡觉，断开连接"),
    ]
    .into_iter()
    .map(|(time, level, message)| LogLine {
        time: time.into(),
        level: level.into(),
        message: message.into(),
    })
    .collect();
    LiveState {
        groups,
        rules,
        logs,
        memory: "22".into(),
        // Same 18-sample fixture as the nodes page, so route cards can flag shaky nodes ("· 抖").
        node_history: histories()
            .into_iter()
            .enumerate()
            .map(|(i, h)| (NODES[i].0.to_string(), h.into_iter().map(|v| v.map(|x| x as u32)).collect()))
            .collect(),
        ..LiveState::default()
    }
}
fn mutate(f: impl FnOnce(&mut LiveState)) {
    LIVE.with(|slot| {
        let live = slot.borrow().clone().expect("preview initialized");
        let mut state = live.lock().expect("live lock");
        f(&mut state);
        state.bump();
    });
    refresh_open();
}
pub fn run() -> Result<(), String> {
    PREVIEW_MODE.with(|p| p.set(true));
    slint::BackendSelector::new()
        .renderer_name(renderer_name())
        .select()
        .map_err(|e| e.to_string())?;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let live = Arc::new(Mutex::new(state()));
    init(tx, live.clone(), "预览配置".into());
    open();
    let args: Vec<_> = std::env::args().collect();
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if let Some(path) = option("--switch-check") {
        return super::switch_check::run(std::path::Path::new(&path), &live);
    }
    if let Some(path) = option("--wave-check") {
        let output = option("--wave-output").ok_or("--wave-check requires --wave-output")?;
        return super::switch_check::waves(
            std::path::Path::new(&path),
            std::path::Path::new(&output),
        );
    }
    if let Some(path) = option("--animation-check") {
        let times = [
            0., 0.01, 0.05, 0.1, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 0.95, 0.99, 1.,
        ];
        let mut curves = Vec::new();
        with_ui(|ui| {
            for (name, points) in [
                ("ease", [0.25, 0.1, 0.25, 1.]),
                ("ease-in-out", [0.42, 0., 0.58, 1.]),
                ("cubic-bezier(.3,.8,.3,1)", [0.3, 0.8, 0.3, 1.]),
            ] {
                let values: Vec<_> = times
                    .iter()
                    .map(|&t| {
                        ui.global::<Anim>()
                            .invoke_bezier(t, points[0], points[1], points[2], points[3])
                    })
                    .collect();
                curves.push(serde_json::json!({"easing": name, "values": values}));
            }
        });
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&serde_json::json!({"times": times, "curves": curves}))
                .unwrap(),
        )
        .map_err(|e| e.to_string())?;
        stop_motion();
        WINDOW.with(|s| s.borrow_mut().take());
        return Ok(());
    }
    let initial = option("--state").unwrap_or("sleep".into());
    action("state", &initial);
    if let Some(page) = option("--page") {
        with_ui(|ui| ui.set_page(page.parse().unwrap_or(0)));
    }
    if args.iter().any(|a| a == "--dark") {
        with_ui(|ui| ui.global::<Theme>().set_dark(true));
    }
    let capture = option("--capture").map(std::path::PathBuf::from);
    if let (Some(script), Some(out)) = (option("--script"), capture.as_ref()) {
        return super::script::run(std::path::Path::new(&script), out, live.clone());
    }
    let benchmark = option("--benchmark").map(std::path::PathBuf::from);
    let mut measurements = Vec::new();
    let mut benchmark_tick = 0usize;
    if let Some(path) = &capture {
        if !path.is_absolute() {
            return Err("--capture requires an absolute directory".into());
        }
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    }
    let timer = slint::Timer::default();
    let mut stage = 0usize;
    let mut phase = false;
    let frames = [
        ("sleep", 0, 0, "v5-sleep"),
        ("ok", 0, 0, "v5-awake"),
        ("ok", 1, 0, "v5-nodes"),
        ("ok", 2, 0, "v5-subscriptions"),
        ("ok", 3, 0, "v5-connections"),
        ("ok", 3, 1, "v5-logs"),
        ("ok", 4, 0, "v5-rules"),
        ("ok", 5, 0, "v5-settings"),
        ("first", 0, 0, "v5-first"),
        ("first", 1, 0, "v5-empty-nodes"),
        ("first", 2, 0, "v5-empty-subscriptions"),
        ("timeout", 0, 0, "v5-timeout"),
        ("offline", 0, 0, "v5-offline"),
        ("sub", 0, 0, "v5-expired"),
        ("port", 0, 0, "v5-port"),
        ("global", 0, 0, "v5-global"),
        ("direct", 0, 0, "v5-direct"),
        ("direct", 4, 0, "v5-paused-rules"),
        ("tray", 0, 0, "v5-tray"),
        ("dark", 0, 0, "v5-dark"),
    ];
    timer.start(slint::TimerMode::Repeated,Duration::from_millis(600),move || {
        if let Some(path) = &capture {
            if stage >= frames.len() {
                verify_interactions();
                println!("preview: {} native screenshots; interaction checks passed", frames.len());
                let _ = slint::quit_event_loop();
                return;
            }
            let (state,page,pane,name) = frames[stage];
            if !phase {
                action("state",state);
                with_ui(|ui| { ui.set_page(page);ui.set_conn_pane(pane);ui.set_reduced_motion(true); });
                phase = true;
            } else {
                with_ui(|ui| {
                    if let Err(e) = capture_frame(ui,&path.join(format!("{name}.ppm"))) {
                        panic!("preview capture {name}: {e}");
                    }
                });
                stage += 1;
                phase = false;
            }
        } else {
            advance();
            if let Some(path)=&benchmark {
                if benchmark_tick.is_multiple_of(10) {
                    let preset=["sleep","ok","global","direct","ok","sleep"][benchmark_tick/10];
                    action("state",preset);
                    with_ui(|ui|ui.set_page((benchmark_tick/10%6) as i32));
                }
                let bytes=crate::status::resident_bytes(std::process::id()).unwrap_or(0);
                measurements.push(serde_json::json!({"second":benchmark_tick as f64*0.6,"rss_bytes":bytes}));
                benchmark_tick+=1;
                #[cfg(target_os="macos")]
                if benchmark_tick==10 {
                    if let Some(dir)=path.parent() {
                        if let Ok(output)=std::process::Command::new("/usr/bin/vmmap").args(["-summary",&std::process::id().to_string()]).output() {
                            let _=std::fs::write(dir.join("memory-map.txt"),output.stdout);
                        }
                    }
                }
                if benchmark_tick==60 {
                    let peak=measurements.iter().filter_map(|v|v["rss_bytes"].as_u64()).max().unwrap_or(0);
                    let report=serde_json::json!({"kind":"native-release-preview","includes_proxy_core":false,"samples":measurements,"peak_mib":peak as f64/1048576.0,"peak_mb":peak as f64/1000000.0,"budget_mb":150,"passed":peak<150_000_000});
                    std::fs::write(path,serde_json::to_vec_pretty(&report).unwrap()).expect("write benchmark report");
                    println!("preview: peak {:.1} MB; budget 150 MB; {}",peak as f64/1000000.0,if peak<150_000_000 {"PASS"} else {"FAIL"});
                    let _=slint::quit_event_loop();
                }
            }
        }
        sync(&live);
    });
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    drop(timer);
    stop_motion();
    WINDOW.with(|slot| slot.borrow_mut().take());
    Ok(())
}
fn capture_frame(ui: &MainWindow, path: &std::path::Path) -> Result<(), String> {
    let frame = ui.window().take_snapshot().map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    write!(out, "P6\n{} {}\n255\n", frame.width(), frame.height()).map_err(|e| e.to_string())?;
    for px in frame.as_slice() {
        out.write_all(&[px.r, px.g, px.b])
            .map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())?;
    println!("preview: captured {}", path.display());
    Ok(())
}
pub fn command(cmd: MenuCommand) {
    match cmd {
        MenuCommand::ToggleRun => {
            let first = WINDOW.with(|w| w.borrow().as_ref().is_some_and(|ui| ui.get_first_run()));
            if first {
                toast("先给它一个窝 · 导入订阅");
                return;
            }
            mutate(|s| {
                s.awake = !s.awake;
                *AWAKE_AT.lock().unwrap() = if s.awake { Some(Instant::now()) } else { None };
                if !s.awake {
                    s.connections.clear();
                    s.down_bps = 0;
                    s.up_bps = 0;
                } else {
                    s.connections = connections(&s.mode);
                }
            });
            with_ui(|ui| {
                if !ui.get_awake() {
                    ui.set_trouble("".into());
                }
                toast(if ui.get_awake() {
                    "已连接 · 继续呼噜"
                } else {
                    "已断开 · 它又睡着了"
                });
            });
        }
        MenuCommand::SetMode(mode) => {
            mutate(|s| {
                s.mode = mode.clone();
                if s.awake {
                    s.connections = connections(&mode);
                }
            });
            toast(match mode.as_str() {
                "global" => "走法 → 全局",
                "direct" => "走法 → 直连",
                _ => "走法 → 分流",
            });
        }
        MenuCommand::Select { group, node } => {
            mutate(|s| {
                if let Some(g) = s.groups.iter_mut().find(|g| g.name == group) {
                    g.now = node.clone();
                    g.now_label = node;
                }
            });
        }
        MenuCommand::TestGroup(group) => {
            mutate(|s| {
                s.testing.insert(group.clone());
            });
            slint::Timer::single_shot(Duration::from_millis(900), move || {
                HISTORY.with(|h| {
                    for (i, row) in h.borrow_mut().iter_mut().enumerate() {
                        row.remove(0);
                        row.push(Some(NODES[i].2 as f32));
                    }
                });
                mutate(|s| {
                    s.testing.remove(&group);
                });
                toast("测速完成");
            });
        }
        MenuCommand::CloseConnections => mutate(|s| s.connections.clear()),
        MenuCommand::Reload => toast("配置已重载"),
        MenuCommand::Quit => {
            let _ = slint::quit_event_loop();
        }
        _ => {}
    }
}
pub fn action(name: &str, value: &str) {
    match name {
        "state" => {
            let awake = !matches!(value, "sleep" | "first" | "dark");
            let trouble = if matches!(value, "timeout" | "offline" | "sub" | "port") {
                value
            } else {
                ""
            };
            let mode = if matches!(value, "global" | "direct") {
                value
            } else {
                "rule"
            };
            with_ui(|ui| {
                ui.set_first_run(value == "first");
                ui.set_trouble(trouble.into());
                ui.set_tray_open(value == "tray");
                ui.global::<Theme>().set_dark(value == "dark");
            });
            mutate(|s| {
                s.awake = awake;
                s.mode = mode.into();
                s.connections = if awake && trouble.is_empty() {
                    connections(mode)
                } else {
                    vec![]
                };
                s.down_bps = if awake && trouble.is_empty() {
                    12_300_000
                } else {
                    0
                };
                s.up_bps = s.down_bps / 4;
                s.history = (0..120)
                    .map(|i| {
                        let down = if awake && trouble.is_empty() {
                            (8.0 + 5.0 * (i as f64 / 11.0).sin() + 2.0 * (i as f64 / 3.0).sin())
                                * 1048576.0
                        } else {
                            0.0
                        };
                        ((down * 0.26) as u64, down as u64)
                    })
                    .collect();
            });
            *AWAKE_AT.lock().unwrap() = awake.then(Instant::now);
            with_ui(|ui| {
                let (title,desc,button,secondary)=match trouble {
                    "timeout"=>("东京 01 没回应了","连续 3 次超过 5 秒没有回音，已经重试过。问题在窝点这边，你的网络是好的。","换到 香港 03 · 21ms","再试一次"),
                    "offline"=>("电脑自己断网了","Wi-Fi 没有连上，跟窝点无关。连上网之后它会自己继续呼噜。","重新检测","打开网络设置"),
                    "sub"=>("橘猫机场 过期了","11 个窝点都停用了。续费后点 更新，或者先换用备用订阅。","去续费 / 更新","换用备用订阅"),
                    "port"=>("7890 被别人占了","ClashX（pid 4127）正在用这个端口，两只猫抢一个窝。换个端口，或者先退出 ClashX。","换成 7891","去改端口"),
                    _=>("","","",""),
                };
                ui.set_trouble_title(title.into());
                ui.set_trouble_desc(desc.into());
                ui.set_trouble_action(button.into());
                ui.set_trouble_secondary(secondary.into());
            });
        }
        "import" => {
            if !value.starts_with("https://") && !value.starts_with("http://") {
                toast("请粘贴有效的订阅链接");
                return;
            }
            toast("闻一闻…");
            slint::Timer::single_shot(Duration::from_millis(1100), || {
                with_ui(|ui| {
                    ui.set_first_run(false);
                    ui.set_page(0);
                });
                refresh_open();
                toast("导入成功 · 橘猫机场 · 11 个窝点");
            });
        }
        "fix" => {
            let trouble = WINDOW.with(|w| w.borrow().as_ref().unwrap().get_trouble().to_string());
            if value == "secondary" && trouble == "port" {
                with_ui(|ui| ui.set_page(5));
                return;
            }
            if value == "primary" && trouble == "sub" {
                with_ui(|ui| ui.set_page(2));
                return;
            }
            if trouble == "timeout" && value == "primary" {
                command(MenuCommand::Select {
                    group: "节点选择".into(),
                    node: "香港 03".into(),
                });
            }
            with_ui(|ui| {
                ui.set_trouble("".into());
                if trouble == "port" {
                    ui.set_mixed_port("7891".into());
                }
            });
            mutate(|s| s.connections = connections(&s.mode));
            toast("已恢复 · 继续呼噜");
        }
        "stable" => {
            let group = LIVE.with(|l| {
                l.borrow().as_ref().unwrap().lock().unwrap().groups[SELECTED_GROUP.with(Cell::get)]
                    .name
                    .clone()
            });
            command(MenuCommand::Select {
                group,
                node: "香港 03".into(),
            });
        }
        "backup" => {
            with_ui(|ui| {
                ui.set_sub_name("小黑猫 · 备用".into());
                ui.set_sub_detail("剩 41 GB".into());
                ui.set_trouble("".into());
            });
            toast("已换用 小黑猫 · 备用");
        }
        "port" => {
            if let Ok(port) = value.parse::<u16>() {
                if port > 0 {
                    with_ui(|ui| ui.set_mixed_port(value.into()));
                }
            }
        }
        "copy" => {
            with_ui(copy_proxy_command);
            toast("已复制终端代理命令");
        }
        "setting" => {
            let mut flag = false;
            with_ui(|ui| {
                flag = match value {
                    "auto-wake" => ui.get_auto_wake(),
                    "auto-swap" => ui.get_auto_swap(),
                    "block-ads" => ui.get_block_ads(),
                    "allow-lan" => ui.get_allow_lan(),
                    "ipv6" => ui.get_ipv6(),
                    _ => false,
                }
            });
            mutate(|state| {
                if let Ok(next) = state.preferences.changed(value, &flag.to_string()) {
                    state.preferences = next;
                }
            });
            toast("习惯已更新");
        }
        "sound" => toast(&format!(
            "呼噜声 → {}",
            match value {
                "off" => "静音",
                "ping" => "一声",
                _ => "一直",
            }
        )),
        "dns" => toast(&format!("DNS → {value}")),
        "update" => {
            with_ui(|ui| ui.set_trouble("".into()));
            toast("订阅已更新");
        }
        "edit" => toast("预览订阅 · 橘猫机场"),
        _ => {}
    }
}
pub fn toggle_system_proxy() {
    SYS.with(|s| s.set(!s.get()));
    refresh_open();
}
pub fn copy_proxy_command(ui: &MainWindow) {
    let port = ui.get_mixed_port();
    copy_proxy_port(&port);
}
pub fn read_clipboard() -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn OpenClipboard(window: *mut c_void) -> i32;
            fn GetClipboardData(format: u32) -> *mut c_void;
            fn CloseClipboard() -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalLock(memory: *mut c_void) -> *mut c_void;
            fn GlobalUnlock(memory: *mut c_void) -> i32;
            fn GlobalSize(memory: *mut c_void) -> usize;
        }
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err("无法读取剪贴板".into());
            }
            let memory = GetClipboardData(13);
            let len = if memory.is_null() {
                0
            } else {
                GlobalSize(memory) / 2
            };
            let pointer = if len > 0 {
                GlobalLock(memory)
            } else {
                std::ptr::null_mut()
            };
            let result = if pointer.is_null() {
                Err("剪贴板没有订阅链接".into())
            } else {
                let wide = std::slice::from_raw_parts(pointer.cast::<u16>(), len.min(8193));
                let end = wide.iter().position(|c| *c == 0).unwrap_or(wide.len());
                let value = if end > 8192 {
                    Err("剪贴板内容过长".into())
                } else {
                    String::from_utf16(&wide[..end]).map_err(|_| "剪贴板文本无效".into())
                };
                GlobalUnlock(memory);
                value
            };
            CloseClipboard();
            result
        }
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("/usr/bin/pbpaste");
        #[cfg(not(target_os = "macos"))]
        let mut command = {
            let mut c = std::process::Command::new("wl-paste");
            c.arg("--no-newline");
            c
        };
        use std::io::Read;
        let mut child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| "无法读取剪贴板")?;
        let mut bytes = Vec::new();
        let read = child
            .stdout
            .take()
            .ok_or("无法读取剪贴板")?
            .take(8193)
            .read_to_end(&mut bytes);
        if bytes.len() > 8192 {
            let _ = child.kill();
            let _ = child.wait();
            return Err("剪贴板内容过长".into());
        }
        let status = child.wait().map_err(|_| "无法读取剪贴板")?;
        if read.is_err() || !status.success() {
            return Err("无法读取剪贴板".into());
        }
        String::from_utf8(bytes).map_err(|_| "剪贴板文本无效".into())
    }
}
pub fn copy_proxy_port(port: &str) {
    #[cfg(not(windows))]
    let text=format!("export https_proxy=http://127.0.0.1:{port} http_proxy=http://127.0.0.1:{port} all_proxy=socks5://127.0.0.1:{port}");
    #[cfg(target_os = "macos")]
    if let Ok(mut child) = std::process::Command::new("/usr/bin/pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
    #[cfg(windows)]
    {
        // Clipboard APIs avoid starting PowerShell for a local UI action.
        use std::ffi::c_void;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn OpenClipboard(window: *mut c_void) -> i32;
            fn EmptyClipboard() -> i32;
            fn SetClipboardData(format: u32, memory: *mut c_void) -> *mut c_void;
            fn CloseClipboard() -> i32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalAlloc(flags: u32, size: usize) -> *mut c_void;
            fn GlobalLock(memory: *mut c_void) -> *mut c_void;
            fn GlobalUnlock(memory: *mut c_void) -> i32;
            fn GlobalFree(memory: *mut c_void) -> *mut c_void;
        }
        let text=format!("$env:HTTP_PROXY='http://127.0.0.1:{port}'; $env:HTTPS_PROXY=$env:HTTP_PROXY; $env:ALL_PROXY='socks5://127.0.0.1:{port}'");
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                let memory = GlobalAlloc(0x2, wide.len() * 2);
                if !memory.is_null() {
                    let pointer = GlobalLock(memory);
                    if !pointer.is_null() {
                        std::ptr::copy_nonoverlapping(
                            wide.as_ptr(),
                            pointer.cast::<u16>(),
                            wide.len(),
                        );
                        GlobalUnlock(memory);
                        EmptyClipboard();
                        if SetClipboardData(13, memory).is_null() {
                            GlobalFree(memory);
                        }
                    } else {
                        GlobalFree(memory);
                    }
                }
                CloseClipboard();
            }
        }
        let _ = text;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = text;
}
fn connections(mode: &str) -> Vec<Conn> {
    [
        ("www.youtube.com", "流媒体", "东京 01"),
        ("api.openai.com", "AI 服务", "洛杉矶 01"),
        ("github.com", "节点选择", "东京 01"),
        ("www.notion.so", "节点选择", "东京 01"),
        ("claude.ai", "AI 服务", "洛杉矶 01"),
        ("www.bilibili.com", "DIRECT", "直连"),
        ("discord.com", "节点选择", "东京 01"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (host, rule, node))| {
        let direct = mode == "direct" || (mode == "rule" && *rule == "DIRECT");
        Conn {
            id: i.to_string(),
            app: if i == 1 {
                "ChatGPT"
            } else if i == 2 {
                "Cursor"
            } else {
                "Chrome"
            }
            .into(),
            host: (*host).into(),
            rule: if mode == "rule" {
                "DOMAIN-SUFFIX".into()
            } else {
                format!("MODE · {mode}")
            },
            policy: if direct {
                "直连".into()
            } else {
                // Same as the live path: mihomo chains[0] (the node actually used), like macOS.
                (*node).to_string()
            },
            kind: if direct { 1 } else { 0 },
            down: format!("{} KB", 420 + i * 91),
            up: format!("{} KB", 24 + i * 12),
            age: format!("{}s", 12 + i * 8),
        }
    })
    .collect()
}
fn advance() {
    TICKS.with(|tick| {
        let t = tick.get() + 1;
        tick.set(t);
        mutate(|s| {
            if !s.awake {
                return;
            }
            let trouble = WINDOW.with(|w| !w.borrow().as_ref().unwrap().get_trouble().is_empty());
            let down = if trouble {
                0
            } else {
                ((9.0 + 5.0 * (t as f32 / 5.0).sin() + 2.0 * (t as f32 / 1.3).sin()) * 1048576.0)
                    as u64
            };
            s.down_bps = down;
            s.up_bps = down / 4;
            s.history.push((down / 4, down));
            if s.history.len() > 120 {
                s.history.remove(0);
            }
            TRAFFIC.with(|v| {
                let (p, d) = v.get();
                let mb = down as f64 / 1048576.0 * 0.6;
                v.set(if s.mode == "direct" {
                    (p, d + mb)
                } else {
                    (p + mb * 0.68, d + mb * 0.32)
                });
            });
        });
    });
}
pub fn decorate(ui: &MainWindow, s: &LiveState) {
    ui.set_sys_proxy(SYS.with(Cell::get));
    // Prototype consistently uses MB/s, including when asleep.
    ui.set_down_value(format!("{:.1}", s.down_bps as f64 / 1048576.0).into());
    ui.set_down_unit("MB/s".into());
    ui.set_up_value(format!("{:.1}", s.up_bps as f64 / 1048576.0).into());
    ui.set_up_unit("MB/s".into());
    ui.set_memory_value(if s.awake { "54.2" } else { "22.0" }.into());
    ui.set_memory_fraction(if s.awake { 54.2 / 128.0 } else { 22.0 / 128.0 });
    ui.set_memory_detail(
        if s.awake {
            "CPU 1.2% · 上限 128 MB"
        } else {
            "CPU 0.1% · 上限 128 MB"
        }
        .into(),
    );
    let (p, d) = TRAFFIC.with(Cell::get);
    let total = p + d;
    ui.set_total_value(format!("{:.2}", total / 1024.0).into());
    ui.set_total_unit("GB".into());
    ui.set_total_detail(
        format!(
            "↓ {:.2} · ↑ {:.2} GB",
            total * 0.8 / 1024.0,
            total * 0.2 / 1024.0
        )
        .into(),
    );
    ui.set_proxy_fraction((p / total) as f32);
    ui.set_proxy_detail(format!("{:.2} GB", p / 1024.0).into());
    ui.set_direct_detail(format!("{:.2} GB", d / 1024.0).into());
    ui.set_blocked_detail("1,284".into());
    ui.set_sub_name("橘猫机场".into());
    ui.set_sub_detail(
        if ui.get_trouble() == "sub" {
            "已过期 · 窝点全部停用"
        } else {
            "剩 38 GB · 127 天后到期"
        }
        .into(),
    );
    let rows = if ui.get_first_run() {
        vec![]
    } else {
        vec![
            SubscriptionRow {
                id: "1".into(),
                title: "橘猫机场".into(),
                detail: "11 个窝点 · 上次更新 刚刚".into(),
                used: "62".into(),
                total: "100".into(),
                fraction: 0.62,
                expiry: "2027-03-12".into(),
                active: true,
                expired: ui.get_trouble() == "sub",
            },
            SubscriptionRow {
                id: "2".into(),
                title: "小黑猫 · 备用".into(),
                detail: "6 个窝点 · 上次更新 昨天".into(),
                used: "8".into(),
                total: "50".into(),
                fraction: 0.168,
                expiry: "2027-06-01".into(),
                active: false,
                expired: false,
            },
        ]
    };
    publish(&SUBSCRIPTIONS, rows, |model| ui.set_subscriptions(model));
    let direct = s.connections.iter().filter(|c| c.kind == 1).count();
    ui.set_conn_detail(
        if s.awake {
            format!("经过窝点 {} · 直连 {direct}", s.connections.len() - direct)
        } else {
            "睡着时没有连接".into()
        }
        .into(),
    );
    let current = &s.groups[0].now_label;
    let mode = match s.mode.as_str() {
        "global" => "全局",
        "direct" => "直连",
        _ => "分流",
    };
    ui.set_status_header_text(if ui.get_first_run() {
        "STATUS 没有窝 · 先导入订阅".into()
    } else if !ui.get_trouble().is_empty() {
        format!("STATUS 卡住了 · {}", ui.get_trouble()).into()
    } else if s.awake {
        format!(
            "STATUS 呼噜中 · {mode} · {current} · {}ms",
            NODES
                .iter()
                .find(|n| n.0 == current)
                .map(|n| n.2)
                .unwrap_or(32)
        )
        .into()
    } else {
        "STATUS 睡着了 · OFF".into()
    });
    ui.set_now_purring_desc(
        if s.awake {
            match s.mode.as_str() {
                "global" => format!("全局：所有流量都经由 {current}。"),
                "direct" => "直连：醒着，但不带路，流量全部直走。".into(),
                _ => format!("分流：国内直走，其余经由 {current}。"),
            }
        } else {
            format!("睡着了：系统代理已还原，内核仍在 {} 端口待命（手动设了代理的程序照常能用）。醒来后按 {mode} 走。", ui.get_mixed_port())
        }
        .into(),
    );
    ui.set_exit_ip(
        if !s.awake || s.mode == "direct" || !ui.get_trouble().is_empty() {
            // macOS exitInfo(): in trouble the IP shown is the local (direct) one.
            "198.51.100.7"
        } else {
            "203.0.113.11"
        }
        .into(),
    );
    ui.set_exit_location(
        if !s.awake {
            "上海 · CN · 本地宽带（睡着时未经过窝点）".into()
        } else if s.mode == "direct" {
            "上海 · CN · 本地宽带，没经过窝点".into()
        } else {
            format!("东京 · JP · 经由 {current} · Hysteria2")
        }
        .into(),
    );
    let peak = s.history.iter().map(|(_, d)| *d).max().unwrap_or(0) as f64 / 1048576.0;
    ui.set_down_peak(format!("60 秒峰值 {peak:.1} MB/s").into());
    ui.set_up_peak(format!("60 秒峰值 {:.1} MB/s", peak * 0.26).into());
    ui.set_rule_stat(format!("{0} 条 · 显示 {0}", s.rules.len()).into());
    let nodes = ui
        .get_spectrum_nodes()
        .iter()
        .map(|mut n| {
            if let Some(i) = NODES.iter().position(|data| data.0 == n.real_name.as_str()) {
                n.jitter = JITTER[i].to_string().into();
                n.loss = LOSS[i].to_string().into();
                n.uptime = UPTIME[i].to_string().into();
                n.shaky = LOSS[i] >= 3 || JITTER[i] >= 20;
            }
            n
        })
        .collect::<Vec<_>>();
    publish(&SPECTRUM_CACHE, nodes, |m| ui.set_spectrum_nodes(m));
    inspect(ui, ui.get_hover_node());
}
pub fn inspect(ui: &MainWindow, id: i32) {
    let nodes = ui.get_spectrum_nodes();
    let n = nodes
        .iter()
        .find(|n| if id < 0 { n.selected } else { n.id == id });
    let Some(n) = n else {
        return;
    };
    ui.global::<S>().set_nd_lbl(
        if n.selected {
            "当前窝点"
        } else {
            "看看这个"
        }
        .into(),
    );
    let i = NODES
        .iter()
        .position(|d| d.0 == n.real_name.as_str())
        .unwrap_or(0);
    ui.set_detail_name(n.name);
    ui.set_detail_kind(format!("{} · {}", n.kind, n.group).into());
    ui.set_detail_delay(n.delay.to_string().into());
    ui.set_detail_jitter(format!("±{}", JITTER[i]).into());
    ui.set_detail_loss(LOSS[i].to_string().into());
    ui.set_detail_uptime(UPTIME[i].to_string().into());
    let shaky = LOSS[i] >= 3 || JITTER[i] >= 20;
    let mild = JITTER[i] >= 10 || LOSS[i] >= 1;
    ui.set_detail_verdict(
        if shaky {
            "常掉线"
        } else if mild {
            "有点抖"
        } else {
            "很稳"
        }
        .into(),
    );
    ui.set_detail_note(
        if shaky {
            "不建议常驻，偶尔用用可以"
        } else if mild {
            "看视频、语音偶尔会卡一下"
        } else {
            "适合长连接、游戏和通话"
        }
        .into(),
    );
    ui.set_detail_alt(
        if mild {
            "同组更稳：香港 03 · 21ms →"
        } else {
            ""
        }
        .into(),
    );
    HISTORY.with(|h| {
        let h = h.borrow();
        let row = &h[i];
        let (path, failures, avg_y) = super::spark_paths(row);
        ui.global::<S>().set_sp_dot(avg_y);
        ui.set_detail_spark(path.into());
        ui.set_detail_failures(if failures.is_empty() {
            "M0 0".into()
        } else {
            failures.into()
        });
    });
}
fn verify_interactions() {
    use slint::platform::{PointerEventButton, WindowEvent};
    with_ui(|ui| {
        ui.set_tray_open(false);
        let width = ui
            .window()
            .size()
            .to_logical(ui.window().scale_factor())
            .width;
        for i in 0..6 {
            let position = slint::LogicalPosition::new(width * (i as f32 + 0.5) / 6.0, 164.0);
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
            assert_eq!(ui.get_page(), i, "native pointer event must select tab");
        }
    });
    action("state", "first");
    command(MenuCommand::ToggleRun);
    with_ui(|ui| assert!(!ui.get_awake(), "first run must block connecting"));
    action("state", "ok");
    command(MenuCommand::SetMode("global".into()));
    with_ui(|ui| assert_eq!(ui.get_mode(), "global"));
    command(MenuCommand::Select {
        group: "节点选择".into(),
        node: "香港 03".into(),
    });
    with_ui(|ui| assert_eq!(ui.get_group_tabs().row_data(0).unwrap().node, "香港 03"));
    with_ui(|ui| ui.invoke_rule_query("openai".into()));
    with_ui(|ui| assert_eq!(ui.get_rules().row_count(), 1));
    with_ui(|ui| ui.invoke_rule_query("".into()));
    command(MenuCommand::CloseConnections);
    with_ui(|ui| assert_eq!(ui.get_conns().row_count(), 0));
    action("state", "timeout");
    action("fix", "primary");
    with_ui(|ui| assert!(ui.get_trouble().is_empty()));
    action("state", "ok");
    command(MenuCommand::ToggleRun);
    with_ui(|ui| assert!(!ui.get_awake()));
    with_ui(|ui| {
        HOLD_ARMED.with(|a| a.set(true));
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        step_hold(ui, 0.3);
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
        assert!(!ui.get_awake(), "short hold must cancel");
        step_hold(ui, 0.0);
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        step_hold(ui, 1.01);
        assert!(ui.get_awake(), "one second Space hold must wake");
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
        step_hold(ui, 0.0);
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
        step_hold(ui, 1.1);
        assert!(ui.get_awake(), "sleep hold takes 1.6 seconds");
        step_hold(ui, 0.51);
        assert!(!ui.get_awake(), "full sleep hold must disconnect");
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
    });
}
