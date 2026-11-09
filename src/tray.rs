use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muda::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use serde_json::Value;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::api::Api;
use crate::core::Core;

const MODES: &[(&str, &str)] = &[("rule", "规则"), ("global", "全局"), ("direct", "直连")];

static QUIT: AtomicBool = AtomicBool::new(false);

pub fn quit_requested() -> bool {
    QUIT.load(Ordering::SeqCst)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorGroup {
    pub name: String,
    pub now: String,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuModel {
    pub mode: String,
    pub groups: Vec<SelectorGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    Open,
    WindowClosed,
    TestGroup(String),
    /// 常用网站真测；true = 手动重测（不受 60 秒节流）
    ProbeSites(bool),
    ToggleRun,
    ToggleSystemProxy,
    ToggleTun,
    UpdateSetting {
        key: String,
        value: String,
        revision: Option<u64>,
    },
    ImportSubscription(String),
    EditSubscription {
        id: u64,
        url: String,
    },
    ImportFile(std::path::PathBuf),
    UpdateSubscription,
    UpdateSubscriptionId(u64),
    UseSubscription(u64),
    Recover(String),
    OpenNetworkSettings,
    SetMode(String),
    Select {
        group: String,
        node: String,
    },
    Reload,
    CloseConnections,
    CloseConnection(String),
    CheckUpdate,
    Quit,
}

#[derive(Clone, Debug, Default)]
pub struct SharedMenu {
    pub generation: u64,
    pub model: MenuModel,
}

pub type MenuActions = HashMap<MenuId, MenuCommand>;

pub fn selectors_from_proxies(root: &Value) -> Vec<SelectorGroup> {
    let Some(proxies) = root.get("proxies").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    let mut global = None;
    for (name, item) in proxies {
        if item.get("type").and_then(Value::as_str) != Some("Selector") {
            continue;
        }
        let now = item
            .get("now")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let options = item
            .get("all")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let group = SelectorGroup {
            name: name.clone(),
            now,
            options,
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

pub fn summary(model: &MenuModel) -> String {
    let modes = MODES
        .iter()
        .map(|(mode, label)| {
            if model.mode == *mode {
                format!("{label}*")
            } else {
                (*label).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let groups = model
        .groups
        .iter()
        .map(|group| {
            let options = group
                .options
                .iter()
                .map(|option| {
                    if option == &group.now {
                        format!("{option}*")
                    } else {
                        option.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {options}", group.name)
        })
        .collect::<Vec<_>>()
        .join(" | ");
    if groups.is_empty() {
        format!("主界面 | {modes} | 重载配置 | 退出")
    } else {
        format!("主界面 | {modes} | {groups} | 重载配置 | 退出")
    }
}

pub fn build_menu(model: &MenuModel) -> Result<(Menu, MenuActions), String> {
    let menu = Menu::new();
    let mut actions = HashMap::new();
    let open = MenuItem::new(menu_label("主界面"), true, None);
    actions.insert(open.id().clone(), MenuCommand::Open);
    menu.append(&open).map_err(|err| err.to_string())?;
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|err| err.to_string())?;
    for (mode, label) in MODES {
        let item = CheckMenuItem::new(menu_label(label), true, model.mode == *mode, None);
        actions.insert(item.id().clone(), MenuCommand::SetMode((*mode).to_string()));
        menu.append(&item).map_err(|err| err.to_string())?;
    }
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|err| err.to_string())?;
    for group in &model.groups {
        let submenu = Submenu::new(menu_label(&group.name), true);
        for option in &group.options {
            let item = CheckMenuItem::new(menu_label(option), true, option == &group.now, None);
            actions.insert(
                item.id().clone(),
                MenuCommand::Select {
                    group: group.name.clone(),
                    node: option.clone(),
                },
            );
            submenu.append(&item).map_err(|err| err.to_string())?;
        }
        menu.append(&submenu).map_err(|err| err.to_string())?;
    }
    if !model.groups.is_empty() {
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|err| err.to_string())?;
    }
    let reload = MenuItem::new(menu_label("重载配置"), true, None);
    actions.insert(reload.id().clone(), MenuCommand::Reload);
    menu.append(&reload).map_err(|err| err.to_string())?;
    let quit = MenuItem::new(menu_label("退出"), true, None);
    actions.insert(quit.id().clone(), MenuCommand::Quit);
    menu.append(&quit).map_err(|err| err.to_string())?;
    Ok((menu, actions))
}

fn menu_label(text: &str) -> String {
    text.replace('&', "&&")
}

pub fn command_for(actions: &MenuActions, id: &MenuId) -> Option<MenuCommand> {
    actions.get(id).cloned()
}

pub fn tray_image() -> tray_icon::Icon {
    tray_image_state(false, false)
}
pub fn tray_image_state(awake: bool, trouble: bool) -> tray_icon::Icon {
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    // Original v5 menubar ear line, including its rounded quadratic corners.
    fn quad(points: &mut Vec<(f32, f32)>, control: (f32, f32), end: (f32, f32)) {
        let start = *points.last().unwrap();
        for i in 1..=12 {
            let t = i as f32 / 12.;
            let u = 1. - t;
            points.push((
                u * u * start.0 + 2. * u * t * control.0 + t * t * end.0,
                u * u * start.1 + 2. * u * t * control.1 + t * t * end.1,
            ));
        }
    }
    let mut outline = vec![(1.5, 15.5), (4.15, 8.3)];
    quad(&mut outline, (4.7, 6.8), (5.55, 8.1));
    outline.push((7.5, 11.));
    quad(&mut outline, (8., 11.8), (8.8, 11.7));
    outline.push((10.3, 11.4));
    quad(&mut outline, (11., 11.2), (11.7, 11.4));
    outline.push((13.2, 11.7));
    quad(&mut outline, (14., 11.8), (14.5, 11.));
    outline.push((16.45, 8.1));
    quad(&mut outline, (17.3, 6.8), (17.85, 8.3));
    outline.push((20.5, 15.5));
    let mut wave = vec![(3., 19.6)];
    for (control, end) in [
        ((5., 18.2), (7., 19.6)),
        ((9., 21.), (11., 19.6)),
        ((13., 18.2), (15., 19.6)),
        ((17., 21.), (19., 19.6)),
    ] {
        quad(&mut wave, control, end);
    }
    let distance = |x: f32, y: f32, a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
        ((x - a.0 - t * dx).powi(2) + (y - a.1 - t * dy).powi(2)).sqrt()
    };
    for y in 0..size {
        for x in 0..size {
            let mut coverage = 0.;
            for sy in 0..3 {
                for sx in 0..3 {
                    let (px, py) = (
                        x as f32 + (sx as f32 + 0.5) / 3.,
                        y as f32 + (sy as f32 + 0.5) / 3.,
                    );
                    let head = outline
                        .windows(2)
                        .any(|s| distance(px, py, s[0], s[1]) < 0.95);
                    let flow =
                        awake && wave.windows(2).any(|s| distance(px, py, s[0], s[1]) < 0.95);
                    let badge =
                        trouble && (px - 19.6).powi(2) + (py - 4.2).powi(2) < 2.6_f32.powi(2);
                    coverage += if flow || badge {
                        1.
                    } else if head {
                        if awake {
                            1.
                        } else {
                            0.45
                        }
                    } else {
                        0.
                    };
                }
            }
            if coverage > 0. {
                let index = ((y * size + x) * 4) as usize;
                #[cfg(not(target_os = "macos"))]
                {
                    rgba[index..index + 3].copy_from_slice(if trouble {
                        &[209, 63, 42]
                    } else if awake {
                        &[255, 138, 76]
                    } else {
                        &[128, 125, 139]
                    });
                }
                rgba[index + 3] = (coverage * 255. / 9.) as u8;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("tray icon")
}

pub async fn serve(
    mut commands: UnboundedReceiver<MenuCommand>,
    api: Api,
    shared: Arc<Mutex<SharedMenu>>,
    live: Arc<Mutex<crate::window::LiveState>>,
    core: Core,
) {
    let mut saved_log_level: Option<String> = None;
    // One consumer preserves click order even when core validation is slow.
    let (setting_tx, mut setting_rx) =
        tokio::sync::mpsc::unbounded_channel::<(String, String, Option<u64>)>();
    let setting_api = api.clone();
    let setting_live = live.clone();
    tokio::spawn(async move {
        while let Some((key, value, revision)) = setting_rx.recv().await {
            if let Err(e) = crate::profile::update(&setting_api, &setting_live, &key, &value).await
            {
                if key == "mixed-port" && e.contains("占用") {
                    let mut s = setting_live.lock().unwrap();
                    s.trouble = "port".into();
                    s.trouble_port = value.parse().unwrap_or(0);
                    s.bump();
                }
                report_error(&setting_live, e);
            } else {
                notice(&setting_live, "习惯已更新");
            }
            if let Some(revision) = revision {
                let _ = slint::invoke_from_event_loop(move || {
                    crate::window::finish_setting(&key, revision)
                });
            }
        }
    });
    if !matches!(
        tokio::time::timeout(Duration::from_secs(15), api.wait_ready()).await,
        Ok(Ok(()))
    ) {
        let mut s = live.lock().unwrap();
        s.trouble = "core".into();
        s.last_error = "代理内核尚未就绪，正在自动重试".into();
        s.bump();
    } else {
        println!(
            "purr: ready shell={} mihomo={} mixed-port={}",
            std::process::id(),
            core.pid(),
            crate::config::MIXED_PORT
        );
        let (auto_wake, system_proxy) = {
            let s = live.lock().expect("live lock");
            (s.preferences.auto_wake, s.preferences.system_proxy)
        };
        if auto_wake && system_proxy {
            if let Err(err) = crate::proxy::enable() {
                eprintln!("purr: system proxy not set: {err}");
            } else {
                crate::window::set_awake(&live, true);
                crate::window::set_place(&live, "查找中".to_string());
            }
        } else if auto_wake {
            crate::window::set_awake(&live, true);
        }
    }
    let traffic_api = api.clone();
    let traffic_live = Arc::clone(&live);
    tokio::spawn(async move { crate::live::traffic_loop(traffic_api, traffic_live).await });
    let log_api = api.clone();
    let log_live = Arc::clone(&live);
    tokio::spawn(async move { crate::live::log_loop(log_api, log_live).await });
    let place_live = Arc::clone(&live);
    tokio::spawn(async move { place_loop(place_live).await });
    let swap_live = live.clone();
    let swap_api = api.clone();
    tokio::spawn(async move {
        health_loop(swap_api, swap_live).await;
    });
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    let mut trim_ticks = 0u32;
    // One warning per loss of TUN, not one every check.
    let mut tun_warned = false;
    let mut cpu_meter = crate::status::CpuMeter::default();
    let (mut terminate, mut interrupt) = exit_signals();
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                trim_ticks = trim_ticks.wrapping_add(1);
                if trim_ticks <= 2 || trim_ticks.is_multiple_of(10) {
                    crate::status::release_unused_memory();
                }
                let shell_sample=crate::status::process_sample(std::process::id());
                let core_sample=crate::status::process_sample(core.pid());
                let shell=shell_sample.memory.unwrap_or(0);
                let kernel=core_sample.memory.unwrap_or(0);
                let cpu_total=shell_sample.cpu.zip(core_sample.cpu).map(|(a,b)|a+b);
                crate::window::set_cpu(&live, cpu_meter.update(&[std::process::id(), core.pid()], cpu_total));
                let label = crate::status::format_memory(shell_sample.memory,core_sample.memory);
                {
                    let mut guard=live.lock().expect("live lock");
                    guard.memory_shell=shell;guard.memory_core=kernel;
                }
                crate::window::set_memory(&live, label);
                refresh(&api, &shared, &live).await;
                if trim_ticks.is_multiple_of(5) {
                    keep_tun(&api, &live, &mut tun_warned).await;
                }
            }
            command = commands.recv() => {
                match command {
                    Some(MenuCommand::Quit) | None => break,
                    Some(MenuCommand::Open) => {
                        if saved_log_level.is_none() {
                            if let Ok(level) = api.log_level().await {
                                saved_log_level = Some(level);
                            }
                            if let Err(err) = api.set_log_level("info").await {
                                eprintln!("purr: log level not raised: {err}");
                            }
                        }
                        let _ = slint::invoke_from_event_loop(crate::window::open);
                    }
                    Some(MenuCommand::WindowClosed) => {
                        if let Some(level) = saved_log_level.take() {
                            if let Err(err) = api.set_log_level(&level).await {
                                eprintln!("purr: log level not restored: {err}");
                            }
                        }
                    }
                    Some(MenuCommand::ProbeSites(force)) => {
                        let go = {
                            let mut g = live.lock().expect("live lock");
                            let fresh = g.sites_at.is_some_and(|t| t.elapsed() < crate::probe::THROTTLE);
                            if g.sites_testing || (!force && fresh) {
                                false
                            } else {
                                g.sites_testing = true;
                                g.bump();
                                true
                            }
                        };
                        if go {
                            let api = api.clone();
                            let live = Arc::clone(&live);
                            tokio::spawn(async move {
                                let mode = live.lock().expect("live lock").mode.clone();
                                let port = crate::config::mixed_port();
                                let sites = crate::probe::probe_all(&api, port, &mode).await;
                                let mut g = live.lock().expect("live lock");
                                // 内核还没就绪（一条路由都没查到）就不计入节流，下次进概览再测
                                if sites.iter().any(|s| !s.group.is_empty()) {
                                    g.sites_at = Some(std::time::Instant::now());
                                }
                                g.sites = sites;
                                g.sites_testing = false;
                                g.bump();
                            });
                        }
                    }
                    Some(MenuCommand::TestGroup(group)) => {
                        let api = api.clone();
                        let live = Arc::clone(&live);
                        tokio::spawn(async move {
                            test_group(&api, &live, &group).await;
                        });
                    }
                    Some(MenuCommand::ToggleRun) => {
                        if let Err(e)=toggle_run(&live,&api).await {report_error(&live,e);}
                    }
                    Some(MenuCommand::ToggleSystemProxy) => {
                        let on={
                            let guard=live.lock().expect("live lock");
                            guard.preferences.system_proxy
                        };
                        let api=api.clone();let live=live.clone();
                        tokio::spawn(async move {
                            if let Err(e)=crate::profile::update(&api,&live,"system-proxy",if on {"false"} else {"true"}).await {report_error(&live,e);}
                        });
                    }
                    Some(MenuCommand::ToggleTun) => {
                        let on=live.lock().expect("live lock").tun;
                        #[cfg(windows)]
                        if !on && !is_windows_admin() {
                            if relaunch_as_admin_for_tun() {
                                break;
                            } else {
                                report_error(&live, "开启虚拟网卡 (TUN) 需要授予管理员权限".into());
                                continue;
                            }
                        }
                        if !on {
                            match api.patch_configs(tun_payload()).await {
                                Ok(()) if tun_confirmed(&api).await => {
                                    crate::window::set_awake(&live,true);
                                    remember_tun(&live,true);
                                    tun_warned=false;
                                    let mut guard=live.lock().expect("live lock");guard.tun=true;guard.last_error.clear();guard.bump();
                                }
                                Ok(()) => {
                                    remember_tun(&live,false);
                                    {let mut guard=live.lock().expect("live lock");guard.tun=false;guard.bump();}
                                    report_error(&live,"虚拟网卡没开起来：内核没有建立 TUN 网卡，请检查管理员权限和 wintun 驱动".into());
                                }
                                Err(_)=>report_error(&live,"TUN 启用失败，请检查管理员权限和内核支持".into()),
                            }
                        } else {
                            match api.patch_configs(serde_json::json!({"tun": {"enable": false}})).await {
                                Ok(())=>{
                                    remember_tun(&live,false);
                                    let mut guard=live.lock().expect("live lock");guard.tun=false;guard.last_error.clear();guard.bump();
                                }
                                Err(_)=>report_error(&live,"虚拟网卡关闭失败，请稍后再试".into()),
                            }
                        }
                    }
                    Some(MenuCommand::UpdateSetting{key,value,revision}) => {
                        let _ = setting_tx.send((key, value, revision));
                    }
                    Some(MenuCommand::ImportFile(path)) => {
                        let api=api.clone();let live=live.clone();
                        if !begin_operation(&live) {continue;}
                        tokio::spawn(async move {
                            let result=crate::profile::import_file(&api,&live,&path).await;
                            let ok=result.is_ok();
                            finish_operation(&live,result,"本地配置已导入");
                            if ok {prepare_import(&api,&live).await;}
                        });
                    }
                    Some(MenuCommand::EditSubscription { id, url }) => {
                        let api=api.clone();let live=live.clone();
                        if !begin_operation(&live) {continue;}
                        tokio::spawn(async move {
                            let result=crate::profile::edit_subscription(&api,&live,id,&url).await;
                            let success=result.is_ok();
                            finish_operation(&live,result,"订阅已保存");
                            if success {
                                let _=slint::invoke_from_event_loop(move || crate::window::finish_subscription_edit(id));
                            }
                        });
                    }
                    Some(MenuCommand::ImportSubscription(url)) => {
                        let api=api.clone();let live=live.clone();
                        if !begin_operation(&live) {continue;}
                        tokio::spawn(async move {
                            let result=crate::profile::import(&api,&live,&url).await;
                            let ok=result.is_ok();
                            finish_operation(&live,result,"导入成功 · 窝点已准备");
                            if ok { prepare_import(&api,&live).await; }
                        });
                    }
                    Some(MenuCommand::UseSubscription(id)) => {
                        let api=api.clone();let live=live.clone();
                        if !begin_operation(&live) {continue;}
                        tokio::spawn(async move {let result=crate::profile::use_subscription(&api,&live,id).await;finish_operation(&live,result,"已换用订阅");});
                    }
                    Some(MenuCommand::UpdateSubscriptionId(id)) => {
                        let api=api.clone();let live=live.clone();
                        if !begin_operation(&live) {continue;}
                        tokio::spawn(async move {let result=crate::profile::update_subscription(&api,&live,id).await;finish_operation(&live,result,"订阅已更新");});
                    }
                    Some(MenuCommand::Recover(action)) => {
                        let api=api.clone();let live=live.clone();
                        tokio::spawn(async move {if let Err(e)=recover(&api,&live,&action).await {report_error(&live,e);}});
                    }
                    Some(MenuCommand::OpenNetworkSettings) => {if let Err(e)=open_network_settings(){report_error(&live,e);}}
                    Some(MenuCommand::CheckUpdate) => {
                        let live = live.clone();
                        notice(&live, "正在与远端核对版本…");
                        tokio::spawn(async move {
                            match crate::status::check_remote_update().await {
                                Ok(info) if info.has_update => {
                                    notice(&live, &format!("发现新版本 v{} · 正在打开下载页", info.latest));
                                    #[cfg(windows)]
                                    let _ = std::process::Command::new("explorer.exe").arg(&info.url).status();
                                    #[cfg(target_os = "macos")]
                                    let _ = std::process::Command::new("/usr/bin/open").arg(&info.url).status();
                                }
                                Ok(info) => {
                                    notice(&live, &format!("当前已是最新版本 PURR v{}", info.current));
                                }
                                Err(e) => {
                                    // No answer is not "up to date".
                                    report_error(&live, format!("检查更新失败：{e}"));
                                }
                            }
                        });
                    }
                    Some(MenuCommand::UpdateSubscription) => {
                        let url=live.lock().expect("live lock").preferences.subscription.clone();
                        if let Some(url)=url {
                            let api=api.clone();let live=live.clone();
                            if !begin_operation(&live) {continue;}
                            tokio::spawn(async move {let result=crate::profile::import(&api,&live,&url).await;finish_operation(&live,result,"订阅已更新");});
                        }
                        refresh(&api,&shared,&live).await;
                    }
                    Some(MenuCommand::CloseConnections) => {
                        if let Err(err) = api.close_connections().await {
                            eprintln!("purr: close connections failed: {err}");
                        }
                        refresh(&api, &shared, &live).await;
                    }
                    Some(MenuCommand::CloseConnection(id)) => {
                        if let Err(e)=api.close_connection(&id).await {report_error(&live,e);}
                        refresh(&api,&shared,&live).await;
                    }
                    Some(command) => {
                        if let Err(err) = apply_command(&api, command).await {
                            report_error(&live,err);
                        } else {
                            let mut s=live.lock().unwrap();s.swap_at=None;if s.trouble=="timeout" {s.trouble.clear();}s.bump();
                        }
                        refresh(&api, &shared, &live).await;
                    }
                }
            }
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
        }
    }
    if let Some(level) = saved_log_level.take() {
        let _ = api.set_log_level(&level).await;
    }
    crate::proxy::restore();
    if let Ok(dir) = crate::config::app_config_dir() {
        let _ = live.lock().unwrap().telemetry.save(&dir);
    }
    core.shutdown().await;
    QUIT.store(true, Ordering::SeqCst);
    let _ = slint::invoke_from_event_loop(|| {
        let _ = slint::quit_event_loop();
    });
}

async fn apply_command(api: &Api, command: MenuCommand) -> Result<(), String> {
    match command {
        MenuCommand::SetMode(mode) => api.set_mode(&mode).await,
        MenuCommand::Select { group, node } => api.select(&group, &node).await,
        MenuCommand::Reload => api.reload().await,
        MenuCommand::Open
        | MenuCommand::WindowClosed
        | MenuCommand::TestGroup(_)
        | MenuCommand::ProbeSites(_)
        | MenuCommand::ToggleRun
        | MenuCommand::ToggleSystemProxy
        | MenuCommand::ToggleTun
        | MenuCommand::UpdateSetting { .. }
        | MenuCommand::ImportSubscription(_)
        | MenuCommand::EditSubscription { .. }
        | MenuCommand::ImportFile(_)
        | MenuCommand::UpdateSubscription
        | MenuCommand::UpdateSubscriptionId(_)
        | MenuCommand::UseSubscription(_)
        | MenuCommand::Recover(_)
        | MenuCommand::OpenNetworkSettings
        | MenuCommand::CheckUpdate
        | MenuCommand::CloseConnections
        | MenuCommand::CloseConnection(_)
        | MenuCommand::Quit => Ok(()),
    }
}

async fn toggle_run(live: &Mutex<crate::window::LiveState>, api: &Api) -> Result<(), String> {
    let (on, sys, tun) = {
        let guard = live.lock().expect("live lock");
        (guard.awake, guard.preferences.system_proxy, guard.tun)
    };
    if on {
        crate::proxy::restore();
        if tun {
            api.patch_configs(serde_json::json!({"tun":{"enable":false}}))
                .await?;
        }
        crate::window::set_awake(live, false);
        {
            let mut guard = live.lock().expect("live lock");
            guard.tun = false;
            guard.bump();
        }
        crate::window::set_place(live, "未连接".to_string());
        return Ok(());
    }
    if sys {
        crate::proxy::enable()?;
    }
    crate::window::set_awake(live, true);
    crate::window::set_place(live, "查找中".to_string());
    Ok(())
}
fn tun_payload() -> serde_json::Value {
    serde_json::json!({
        "tun": {
            "enable": true,
            "stack": "system",
            "auto-route": true,
            "auto-detect-interface": true,
            "dns-hijack": ["any:53"]
        }
    })
}

/// The core accepts the PATCH before the adapter exists; read it back for up to 2 s.
async fn tun_confirmed(api: &Api) -> bool {
    for _ in 0..8 {
        if api.tun_enabled().await.unwrap_or(false) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

fn remember_tun(live: &Mutex<crate::window::LiveState>, on: bool) {
    let mut guard = live.lock().expect("live lock");
    if guard.preferences.tun == on {
        return;
    }
    guard.preferences.tun = on;
    match crate::config::app_config_dir() {
        Ok(dir) => {
            if let Err(e) = guard.preferences.save(&dir) {
                eprintln!("purr: tun preference not saved: {e}");
            }
        }
        Err(e) => eprintln!("purr: tun preference not saved: {e}"),
    }
}

fn can_enable_tun() -> bool {
    #[cfg(windows)]
    {
        is_windows_admin()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Keeps the TUN switch honest: the UI shows what the core actually runs, and a TUN the user
/// turned on comes back after a subscription update, a reload or a core restart dropped it.
async fn keep_tun(api: &Api, live: &Mutex<crate::window::LiveState>, warned: &mut bool) {
    let (awake, want, shown) = {
        let guard = live.lock().expect("live lock");
        (guard.awake, guard.preferences.tun, guard.tun)
    };
    let Ok(actual) = api.tun_enabled().await else {
        return;
    };
    if actual {
        *warned = false;
        if !shown {
            let mut guard = live.lock().expect("live lock");
            guard.tun = true;
            guard.bump();
        }
        return;
    }
    if shown {
        let mut guard = live.lock().expect("live lock");
        guard.tun = false;
        guard.bump();
    }
    if !(awake && want) {
        return;
    }
    if can_enable_tun()
        && api.patch_configs(tun_payload()).await.is_ok()
        && tun_confirmed(api).await
    {
        *warned = false;
        {
            let mut guard = live.lock().expect("live lock");
            guard.tun = true;
            guard.bump();
        }
        notice(live, "虚拟网卡已重新打开");
        return;
    }
    if !*warned {
        *warned = true;
        let message = if can_enable_tun() {
            "虚拟网卡掉了，自动重开失败，请检查 wintun 驱动"
        } else {
            "虚拟网卡掉了：需要管理员权限，点「虚拟网卡」重新开启"
        };
        report_error(live, message.into());
    }
}

fn report_error(live: &Mutex<crate::window::LiveState>, message: String) {
    let mut state = live.lock().expect("live lock");
    state.last_error = message.clone();
    state.notice = message;
    state.notice_id = state.notice_id.wrapping_add(1);
    state.bump();
}
fn begin_operation(live: &Mutex<crate::window::LiveState>) -> bool {
    let mut s = live.lock().expect("live lock");
    if s.busy {
        return false;
    }
    s.busy = true;
    s.last_error.clear();
    s.bump();
    true
}
fn finish_operation(
    live: &Mutex<crate::window::LiveState>,
    result: Result<(), String>,
    message: &str,
) {
    {
        let mut s = live.lock().expect("live lock");
        s.busy = false;
        s.bump();
    }
    match result {
        Ok(()) => notice(live, message),
        Err(e) => report_error(live, e),
    }
}
fn notice(live: &Mutex<crate::window::LiveState>, message: &str) {
    let mut s = live.lock().expect("live lock");
    s.notice = message.into();
    s.notice_id = s.notice_id.wrapping_add(1);
    s.last_error.clear();
    s.bump();
}
fn network_available() -> bool {
    crate::status::network_available()
}
fn open_network_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.Network-Settings.extension")
        .status();
    #[cfg(windows)]
    let status = std::process::Command::new("explorer.exe")
        .arg("ms-settings:network-status")
        .status();
    #[cfg(not(any(windows, target_os = "macos")))]
    let status = std::process::Command::new("nm-connection-editor").status();
    status
        .map_err(|_| "无法打开系统网络设置".to_string())
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err("无法打开系统网络设置".into())
            }
        })
}
async fn recover(
    api: &Api,
    live: &Mutex<crate::window::LiveState>,
    action: &str,
) -> Result<(), String> {
    let trouble = live.lock().expect("live lock").trouble.clone();
    match trouble.as_str() {
        "port" => {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| "找不到可用端口")?;
            let port = probe.local_addr().map_err(|_| "找不到可用端口")?.port();
            drop(probe);
            crate::profile::update(api, live, "mixed-port", &port.to_string()).await?;
        }
        "sub" => {
            let id = {
                let s = live.lock().expect("live lock");
                s.preferences
                    .subscriptions
                    .iter()
                    .find(|sub| {
                        Some(&sub.url) != s.preferences.subscription.as_ref() && !sub.exhausted()
                    })
                    .map(|s| s.id)
            };
            if let Some(id) = id {
                crate::profile::use_subscription(api, live, id).await?;
            } else {
                return Err("还没有可用的备用订阅，请先导入".into());
            }
        }
        "timeout" if action == "primary" => {
            select_available(api, live).await?;
        }
        "timeout" => {
            let name = {
                let s = live.lock().expect("live lock");
                s.groups
                    .iter()
                    .find(|g| g.name != "GLOBAL")
                    .map(|g| g.now.clone())
                    .ok_or("没有可测速的窝点")?
            };
            let delay = api
                .delay(&name)
                .await
                .map_err(|_| "它还没有回应，可以换一个窝点")?;
            crate::window::set_delay(live, &name, format!("{delay}ms"));
        }
        "offline" => {
            if !network_available() {
                return Err("网络还没有恢复，请先连接 Wi-Fi 或有线网络".into());
            }
        }
        _ => {
            api.version()
                .await
                .map_err(|_| "内核还没有回应，正在自动重试")?;
        }
    }
    {
        let mut s = live.lock().expect("live lock");
        s.trouble.clear();
        s.swap_at = None;
        s.bump();
    }
    notice(live, "它回应了 · 继续呼噜");
    Ok(())
}
async fn select_available(api: &Api, live: &Mutex<crate::window::LiveState>) -> Result<(), String> {
    use futures_util::{stream, StreamExt};
    let group = {
        let s = live.lock().expect("live lock");
        s.groups
            .iter()
            .find(|g| {
                g.kind == "select"
                    && if s.mode == "global" {
                        g.name == "GLOBAL"
                    } else {
                        g.name != "GLOBAL"
                    }
            })
            .cloned()
            .ok_or("没有可切换的节点组")?
    };
    let names = group
        .nodes
        .iter()
        .filter(|n| n.name != group.now && !crate::present::skip_delay(&n.kind))
        .map(|n| n.name.clone())
        .collect::<Vec<_>>();
    let mut candidates = stream::iter(names.into_iter().map(|name| {
        let api = api.clone();
        async move {
            let d = api.delay(&name).await.ok();
            (name, d)
        }
    }))
    .buffer_unordered(4);
    let mut best: Option<(String, u32)> = None;
    while let Some((name, delay)) = candidates.next().await {
        crate::window::set_delay(
            live,
            &name,
            delay
                .map(|d| format!("{d}ms"))
                .unwrap_or_else(|| "超时".into()),
        );
        if let Some(delay) = delay {
            if best.as_ref().is_none_or(|(_, d)| delay < *d) {
                best = Some((name, delay));
            }
        }
    }
    let (name, _) = best.ok_or("这个组暂时没有可用的窝点，请更新订阅或重试")?;
    let unchanged = {
        let s = live.lock().expect("live lock");
        s.groups
            .iter()
            .any(|g| g.name == group.name && g.now == group.now)
    };
    if !unchanged {
        return Err("节点已由其他操作切换".into());
    }
    api.select(&group.name, &name).await?;
    {
        let mut s = live.lock().expect("live lock");
        s.trouble.clear();
        s.swap_at = None;
        s.bump();
    }
    notice(
        live,
        &format!("已换到 {}", crate::present::display_name(&name)),
    );
    crate::window::push_log(
        live,
        "purr",
        &format!("换窝到 {}", crate::present::display_name(&name)),
    );
    Ok(())
}

async fn place_loop(live: Arc<Mutex<crate::window::LiveState>>) {
    let mut last = tokio::time::Instant::now() - Duration::from_secs(60);
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let (awake, place) = {
            let guard = live.lock().expect("live lock");
            (guard.awake, guard.place.clone())
        };
        if !awake {
            // Asleep: show the real local (direct) IP like macOS exitInfo(), looked up once per nap.
            if place == "未连接" {
                crate::window::set_place(&live, "查找中…".to_string());
                let next = match crate::status::lookup_direct_place().await {
                    Some(p) => format!("{p}（睡着时未经过窝点）"),
                    None => "— · 睡着时未经过窝点".to_string(),
                };
                if !live.lock().expect("live lock").awake {
                    crate::window::set_place(&live, next);
                }
            }
            continue;
        }
        let due = place == "查找中" || last.elapsed() >= Duration::from_secs(20);
        if !due {
            continue;
        }
        let next = crate::status::lookup_place().await;
        if live.lock().expect("live lock").awake {
            crate::window::set_place(&live, next);
            last = tokio::time::Instant::now();
        }
    }
}
async fn health_loop(api: Api, live: Arc<Mutex<crate::window::LiveState>>) {
    let mut failed = 0u8;
    let mut previous = String::new();
    let mut last_probe = std::time::Instant::now() - Duration::from_secs(20);
    let mut last_save = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let fixture = crate::config::app_config_dir()
            .is_ok_and(|d| d.join("purr-validation-fixture").exists());
        let mut due_swap = false;
        {
            let mut s = live.lock().expect("live lock");
            s.telemetry.roll(crate::subscriptions::now());
            if s.preferences
                .subscriptions
                .iter()
                .find(|sub| Some(&sub.url) == s.preferences.subscription.as_ref())
                .is_some_and(|sub| sub.exhausted())
            {
                s.trouble = "sub".into();
                s.swap_at = None;
            } else if s.trouble == "sub" {
                s.trouble.clear();
            }
            if s.awake && !fixture {
                if !network_available() && (s.trouble.is_empty() || s.trouble == "offline") {
                    s.trouble = "offline".into();
                } else if s.trouble == "offline" {
                    s.trouble.clear();
                    s.notice = "网络回来了 · 继续呼噜".into();
                    s.notice_id = s.notice_id.wrapping_add(1);
                }
            }
            if s.swap_at.is_some_and(|t| t <= std::time::Instant::now()) {
                due_swap = s.awake && s.preferences.auto_swap && s.trouble == "timeout";
                s.swap_at = None;
            }
            if last_save.elapsed() >= Duration::from_secs(60) {
                if let Ok(dir) = crate::config::app_config_dir() {
                    let _ = s.telemetry.save(&dir);
                }
                last_save = std::time::Instant::now();
            }
            s.bump();
        }
        if due_swap {
            if let Err(e) = select_available(&api, &live).await {
                report_error(&live, e);
            }
            failed = 0;
            continue;
        }
        if last_probe.elapsed() < Duration::from_secs(20) {
            continue;
        }
        last_probe = std::time::Instant::now();
        let group = {
            let state = live.lock().expect("live lock");
            if !state.awake
                || state.mode == "direct"
                || (!state.trouble.is_empty() && state.trouble != "timeout")
            {
                None
            } else {
                // The group the overview and tray show (top-level select, e.g. "🌍 出口").
                crate::present::main_group(&state.groups, &state.mode)
                    .filter(|g| g.kind == "select")
                    .cloned()
            }
        };
        let Some(group) = group else {
            failed = 0;
            continue;
        };
        if group.now != previous {
            failed = 0;
            previous = group.now.clone();
        }
        if group
            .nodes
            .iter()
            .find(|n| n.name == group.now)
            .is_none_or(|n| !crate::present::probe_delay(&n.kind))
        {
            continue;
        }
        match api.delay(&group.now).await {
            Ok(delay) => {
                failed = 0;
                crate::window::set_delay(&live, &group.now, format!("{delay}ms"));
                let mut s = live.lock().unwrap();
                if s.trouble == "timeout" {
                    s.trouble.clear();
                    s.swap_at = None;
                    s.bump();
                }
            }
            Err(_) => {
                failed += 1;
                crate::window::set_delay(&live, &group.now, "超时".into());
            }
        }
        if failed < 3 {
            continue;
        }
        {
            let mut s = live.lock().unwrap();
            s.trouble = "timeout".into();
            s.swap_at = if s.preferences.auto_swap {
                Some(std::time::Instant::now() + Duration::from_secs(8))
            } else {
                None
            };
            s.bump();
        }
        failed = 0;
    }
}

async fn test_group(api: &Api, live: &Mutex<crate::window::LiveState>, group: &str) {
    use futures_util::{stream, StreamExt};
    let Some(nodes) = crate::window::begin_test(live, group) else {
        return;
    };
    let names = nodes
        .into_iter()
        .filter(|(_, kind)| crate::present::probe_delay(kind))
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    for name in &names {
        crate::window::set_delay(live, name, "…".into());
    }
    let mut probes = stream::iter(names.into_iter().map(|name| async move {
        let delay = api
            .delay(&name)
            .await
            .map(|v| format!("{v}ms"))
            .unwrap_or_else(|_| "超时".into());
        (name, delay)
    }))
    .buffer_unordered(12);
    while let Some((name, delay)) = probes.next().await {
        crate::window::set_delay(live, &name, delay);
    }
    crate::window::set_testing(live, group, false);
}

async fn prepare_import(api: &Api, live: &Mutex<crate::window::LiveState>) {
    let active =
        {
            let state = live.lock().unwrap();
            if state.preferences.subscriptions.iter().any(|sub| {
                Some(&sub.url) == state.preferences.subscription.as_ref() && sub.exhausted()
            }) {
                return;
            }
            state.preferences.subscription.clone()
        };
    let Ok(proxies) = api.proxies().await else {
        return;
    };
    let groups = crate::present::proxy_groups(&proxies);
    let mode = live.lock().unwrap().mode.clone();
    crate::window::set_directory(live, &mode, groups.clone());
    let Some(group) = groups
        .iter()
        .find(|g| {
            g.name != "GLOBAL"
                && g.kind == "select"
                && g.nodes.iter().any(|n| !crate::present::skip_delay(&n.kind))
        })
        .cloned()
    else {
        return;
    };
    test_group(api, live, &group.name).await;
    let best = {
        let s = live.lock().unwrap();
        if s.preferences.subscription != active
            || !s
                .groups
                .iter()
                .any(|g| g.name == group.name && g.now == group.now)
        {
            return;
        }
        group
            .nodes
            .iter()
            .filter(|n| !crate::present::skip_delay(&n.kind))
            .filter_map(|n| {
                s.delays
                    .get(&n.name)?
                    .strip_suffix("ms")?
                    .parse::<u32>()
                    .ok()
                    .map(|ms| (n.name.clone(), ms))
            })
            .min_by_key(|(_, ms)| *ms)
            .map(|(name, _)| name)
    };
    if let Some(name) = best {
        if api.select(&group.name, &name).await.is_ok() {
            notice(
                live,
                &format!(
                    "测速完成 · 最暖的窝：{}",
                    crate::present::display_name(&name)
                ),
            );
        }
    }
}
async fn refresh(api: &Api, shared: &Mutex<SharedMenu>, live: &Mutex<crate::window::LiveState>) {
    let mode = match api.mode().await {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("purr: menu refresh failed: {err}");
            let mut s = live.lock().unwrap();
            if s.awake && s.trouble.is_empty() {
                s.trouble = "core".into();
                s.bump();
            }
            return;
        }
    };
    {
        let mut s = live.lock().unwrap();
        if s.trouble == "core" {
            s.trouble.clear();
            s.bump();
        }
    }
    let proxies = match api.proxies().await {
        Ok(proxies) => proxies,
        Err(err) => {
            eprintln!("purr: menu refresh failed: {err}");
            return;
        }
    };
    let next = MenuModel {
        mode: mode.clone(),
        groups: selectors_from_proxies(&proxies),
    };
    crate::window::set_directory(live, &mode, crate::present::proxy_groups(&proxies));
    if let Ok(value) = api.connections().await {
        crate::window::set_connections(live, &value);
    }
    if let Ok(value) = api.rules().await {
        crate::window::set_rules(live, &value);
    }
    let mut guard = shared.lock().expect("menu lock");
    if guard.model != next {
        guard.model = next;
        guard.generation = guard.generation.wrapping_add(1);
    }
}

#[cfg(unix)]
fn exit_signals() -> (tokio::signal::unix::Signal, tokio::signal::unix::Signal) {
    use tokio::signal::unix::{signal, SignalKind};
    let terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
    let interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler");
    (terminate, interrupt)
}

#[cfg(windows)]
fn exit_signals() -> (WindowsExitSignal, WindowsExitSignal) {
    (
        WindowsExitSignal::Break(tokio::signal::windows::ctrl_break().ok()),
        WindowsExitSignal::Interrupt(tokio::signal::windows::ctrl_c().ok()),
    )
}
#[cfg(windows)]
enum WindowsExitSignal {
    Break(Option<tokio::signal::windows::CtrlBreak>),
    Interrupt(Option<tokio::signal::windows::CtrlC>),
}
#[cfg(windows)]
impl WindowsExitSignal {
    async fn recv(&mut self) {
        match self {
            Self::Break(Some(signal)) => {
                signal.recv().await;
            }
            Self::Interrupt(Some(signal)) => {
                signal.recv().await;
            }
            _ => std::future::pending().await, // GUI launches may have no attached console.
        }
    }
}

/// Whether TUN can be enabled without asking for elevation first (macOS shows "需授权" otherwise).
pub(crate) fn tun_authorized() -> bool {
    #[cfg(windows)]
    {
        is_windows_admin()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn is_windows_admin() -> bool {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn IsUserAnAdmin() -> i32;
    }
    unsafe { IsUserAnAdmin() != 0 }
}

#[cfg(windows)]
fn relaunch_as_admin_for_tun() -> bool {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_cmd: i32,
        ) -> isize;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let verb: Vec<u16> = "runas".encode_utf16().chain(Some(0)).collect();
    let file: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let params: Vec<u16> = "--enable-tun".encode_utf16().chain(Some(0)).collect();
    let ret = unsafe {
        ShellExecuteW(
            0,
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        )
    };
    ret > 32
}

pub fn click_sender(
    actions: Arc<Mutex<MenuActions>>,
    commands: UnboundedSender<MenuCommand>,
) -> impl Fn(muda::MenuEvent) + Send + Sync + 'static {
    move |event| {
        let command = command_for(&actions.lock().expect("menu lock"), &event.id);
        if let Some(command) = command {
            let _ = commands.send(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{command_for, selectors_from_proxies, MenuCommand};
    use muda::MenuId;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn selector_groups_skip_fallback_and_keep_global_last() {
        let value = json!({
            "proxies": {
                "GLOBAL": {"type": "Selector", "now": "DIRECT", "all": ["DIRECT", "REJECT"]},
                "出口": {"type": "Selector", "now": "A", "all": ["A", "B"]},
                "US": {"type": "Fallback", "now": "A", "all": ["A", "B"]},
                "节点": {"type": "Shadowsocks", "now": "", "all": []}
            }
        });
        let groups = selectors_from_proxies(&value);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "出口");
        assert_eq!(groups[0].now, "A");
        assert_eq!(groups[0].options, ["A", "B"]);
        assert_eq!(groups[1].name, "GLOBAL");
    }

    #[test]
    fn command_lookup_uses_menu_id() {
        let id = MenuId::new("mode-rule");
        let mut actions = HashMap::new();
        actions.insert(id.clone(), MenuCommand::SetMode("rule".into()));
        assert_eq!(
            command_for(&actions, &id),
            Some(MenuCommand::SetMode("rule".into()))
        );
    }
}
