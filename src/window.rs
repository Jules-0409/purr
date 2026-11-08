// SPDX-License-Identifier: GPL-3.0-only

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slint::winit_030::{winit, WinitWindowAccessor};
use slint::Model;
use slint::ModelRc;
use slint::VecModel;
use tokio::sync::mpsc::UnboundedSender;

use crate::present::{self, Conn, ProxyGroup, Rule};
use crate::tray::MenuCommand;

mod design;
pub mod disclaimer;
mod fixture;
mod popover;
mod preview;
mod script;
mod switch_check;
pub use fixture::{driving as runtime_ui_driving, tick as verify_runtime_tick};
pub use popover::toggle as toggle_tray;
use popover::{apply_tray, tray_nodes};
pub use preview::run as preview;
pub fn renderer_name() -> String {
    std::env::var("PURR_RENDERER").unwrap_or_else(|_| "software".into())
}

slint::slint! {
    export { MainWindow, Theme, IconClock, TrayWindow, TrayNode, S, Anim, AsciiRow, SiteRow } from "ui/app.slint";
    export { SwitchCheckWindow } from "ui/switch-check.slint";
    export { DisclaimerWindow } from "ui/disclaimer.slint";
}

thread_local! {
    static COMMANDS: RefCell<Option<UnboundedSender<MenuCommand>>> = const { RefCell::new(None) };
    static LIVE: RefCell<Option<Arc<Mutex<LiveState>>>> = const { RefCell::new(None) };
    static WINDOW: RefCell<Option<MainWindow>> = const { RefCell::new(None) };
    static MOTION: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static TOAST_TIMER: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static SEEN: Cell<u64> = const { Cell::new(0) };
    static LAST_TICK: Cell<Option<Instant>> = const { Cell::new(None) };
    static HOLD_ARMED: Cell<bool> = const { Cell::new(true) };
    static RULE_QUERY: RefCell<String> = const { RefCell::new(String::new()) };
    static CONN_QUERY: RefCell<String> = const { RefCell::new(String::new()) };
    static LOG_FILTER: RefCell<String> = const { RefCell::new(String::new()) };
    static SELECTED_GROUP: Cell<usize> = const { Cell::new(0) };
    static SPECTRUM_CACHE: RefCell<Option<Rc<VecModel<NodeItem>>>> = const { RefCell::new(None) };
    static LIST_CACHE: RefCell<Option<Rc<VecModel<NodeItem>>>> = const { RefCell::new(None) };
    static GROUP_TABS_CACHE: RefCell<Option<Rc<VecModel<GroupTab>>>> = const { RefCell::new(None) };
    static CONNS: RefCell<Option<Rc<VecModel<ConnRow>>>> = const { RefCell::new(None) };
    static RULES: RefCell<Option<Rc<VecModel<RuleRow>>>> = const { RefCell::new(None) };
    static RULE_INPUT: RefCell<Option<RuleInput>> = const { RefCell::new(None) };
    static LOGS: RefCell<Option<Rc<VecModel<LogRow>>>> = const { RefCell::new(None) };
    static PREVIEW_MODE: Cell<bool> = const { Cell::new(false) };
    static LAST_NOTICE: Cell<u64> = const { Cell::new(0) };
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    static SUBSCRIPTIONS: RefCell<Option<Rc<VecModel<SubscriptionRow>>>> = const { RefCell::new(None) };
    static STABLE_NODE: RefCell<Option<(String,String)>> = const { RefCell::new(None) };
    static TRAY_WINDOW: RefCell<Option<TrayWindow>> = const { RefCell::new(None) };
    static TRAY_TIMER: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    static PAGE_STARTED: Cell<Instant> = Cell::new(Instant::now());
    static ASCII_KEY: RefCell<String> = const { RefCell::new(String::new()) };
    static SHAKE_STARTED: Cell<Instant> = Cell::new(Instant::now());
    static AUTO_TESTED: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
    static SETTING_REVISION: Cell<u64> = const { Cell::new(0) };
    static PENDING_SETTINGS: RefCell<HashMap<String, (u64, bool)>> = RefCell::new(HashMap::new());
}

static AWAKE_AT: Mutex<Option<Instant>> = Mutex::new(None);

struct RuleInput {
    rules: Arc<[Rule]>,
    query: String,
    hits: HashMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct LiveState {
    pub generation: u64,
    pub awake: bool,
    pub up_bps: u64,
    pub down_bps: u64,
    /// True while the core's /traffic stream is delivering; speeds show "—" otherwise.
    pub traffic_live: bool,
    pub flow: f32,
    pub history: Vec<(u64, u64)>,
    pub last_sample: Option<Instant>,
    pub mode: String,
    pub groups: Vec<ProxyGroup>,
    pub delays: HashMap<String, String>,
    pub testing: HashSet<String>,
    pub logs: Vec<LogLine>,
    pub memory: String,
    pub place: String,
    pub session_up: String,
    pub session_down: String,
    pub connections: Vec<Conn>,
    pub rules: Arc<[Rule]>,
    pub config_dir: String,
    pub preferences: crate::settings::Preferences,
    pub tun: bool,
    pub last_error: String,
    pub node_history: HashMap<String, Vec<Option<u32>>>,
    pub memory_shell: u64,
    pub memory_core: u64,
    pub route_down: [u64; 2],
    pub blocked_connections: u64,
    pub trouble: String,
    pub trouble_port: u16,
    pub swap_at: Option<Instant>,
    pub busy: bool,
    pub notice: String,
    pub notice_id: u64,
    pub telemetry: crate::telemetry::Telemetry,
    pub last_nap: u64,
    pub sites: Vec<crate::probe::SiteProbe>,
    pub sites_at: Option<Instant>,
    pub sites_testing: bool,
}

impl Default for LiveState {
    fn default() -> Self {
        Self {
            generation: 0,
            awake: false,
            up_bps: 0,
            down_bps: 0,
            traffic_live: false,
            flow: 0.0,
            history: Vec::new(),
            last_sample: None,
            mode: "rule".to_string(),
            groups: Vec::new(),
            delays: HashMap::new(),
            testing: HashSet::new(),
            logs: Vec::new(),
            memory: "Purr — · 内核 —".to_string(),
            place: "未连接".to_string(),
            session_up: "0 B".to_string(),
            session_down: "0 B".to_string(),
            connections: Vec::new(),
            rules: Arc::from([]),
            config_dir: String::new(),
            preferences: crate::settings::Preferences::default(),
            tun: false,
            last_error: String::new(),
            node_history: HashMap::new(),
            memory_shell: 0,
            memory_core: 0,
            route_down: [0; 2],
            blocked_connections: 0,
            trouble: String::new(),
            trouble_port: 0,
            swap_at: None,
            busy: false,
            notice: String::new(),
            notice_id: 0,
            telemetry: Default::default(),
            last_nap: 0,
            sites: Vec::new(),
            sites_at: None,
            sites_testing: false,
        }
    }
}

impl LiveState {
    pub fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

pub fn init(
    commands: UnboundedSender<MenuCommand>,
    live: Arc<Mutex<LiveState>>,
    config_dir: String,
) {
    COMMANDS.with(|slot| *slot.borrow_mut() = Some(commands));
    live.lock().expect("live lock").config_dir = config_dir;
    LIVE.with(|slot| *slot.borrow_mut() = Some(live));
}

pub fn open() {
    let created = WINDOW.with(|slot| {
        if slot.borrow().is_some() {
            if let Some(ui) = slot.borrow().as_ref() {
                let _ = ui.show();
            }
            return false;
        }
        let ui = match MainWindow::new() {
            Ok(ui) => ui,
            Err(err) => {
                eprintln!("purr: window failed: {err}");
                return false;
            }
        };
        ui.set_preview(PREVIEW_MODE.with(Cell::get));
        ui.set_reduced_motion(crate::status::reduced_motion());
        ui.window().on_winit_window_event(|_, event| {
            if let winit::event::WindowEvent::DroppedFile(path) = event {
                if !PREVIEW_MODE.with(Cell::get) {
                    send(MenuCommand::ImportFile(path.clone()));
                }
            }
            // Maximize / restore / resize: square corners when maximized, and the Windows 10
            // window region follows the new size.
            #[cfg(windows)]
            if matches!(
                event,
                winit::event::WindowEvent::Resized(_) | winit::event::WindowEvent::ScaleFactorChanged { .. }
            ) {
                slint::Timer::single_shot(Duration::ZERO, || with_ui(apply_frame_shape));
            }
            slint::winit_030::EventResult::Propagate
        });
        #[cfg(windows)]
        {
            ui.global::<Theme>().set_shortcut("Ctrl+".into());
            ui.global::<Theme>().set_mono("Consolas".into());
            // SF Mono advances 0.619 em, Consolas 0.550 em: track the difference so widths match macOS.
            ui.global::<Theme>().set_mono_track(0.069);
            ui.global::<Theme>().set_sans("Segoe UI".into());
        }
        ui.on_inspect_node(|id| {
            if PREVIEW_MODE.with(Cell::get) {
                with_ui(|ui| preview::inspect(ui, id));
            } else {
                let state = LIVE.with(|l| l.borrow().as_ref().unwrap().lock().unwrap().clone());
                with_ui(|ui| inspect_live(ui, &state, id));
            }
        });
        ui.on_action(|action, value| {
            if PREVIEW_MODE.with(Cell::get) {
                preview::action(action.as_str(), value.as_str());
            } else {
                with_ui(|ui| match action.as_str() {
                    "import" => {
                        if let Ok(id) = ui.get_editing_subscription().parse() {
                            send(MenuCommand::EditSubscription {
                                id,
                                url: value.to_string(),
                            });
                        } else {
                            send(MenuCommand::ImportSubscription(value.to_string()));
                        }
                    }
                    "cancel-edit" => {
                        ui.set_editing_subscription("".into());
                        ui.set_sub_editor_url("".into());
                    }
                    "paste" => match preview::read_clipboard() {
                        Ok(text) => {
                            let url = text.trim();
                            ui.set_sub_editor_url(url.into());
                            send(MenuCommand::ImportSubscription(url.into()));
                        }
                        Err(e) => toast(&e),
                    },
                    "update" => send(MenuCommand::UpdateSubscription),
                    "check-update" => send(MenuCommand::CheckUpdate),
                    "port" => {
                        if value.parse::<u16>().is_ok_and(|p| p > 0) {
                            send(MenuCommand::UpdateSetting {
                                key: "mixed-port".into(),
                                value: value.to_string(),
                                revision: None,
                            });
                        } else {
                            toast("端口必须为 1–65535");
                        }
                    }
                    "setting" => {
                        let flag = match value.as_str() {
                            "auto-wake" => ui.get_auto_wake(),
                            "auto-swap" => ui.get_auto_swap(),
                            "block-ads" => ui.get_block_ads(),
                            "close-to-tray" => ui.get_close_to_tray(),
                            "allow-lan" => ui.get_allow_lan(),
                            "ipv6" => ui.get_ipv6(),
                            _ => false,
                        };
                        send(MenuCommand::UpdateSetting {
                            key: value.to_string(),
                            value: flag.to_string(),
                            revision: Some(begin_setting(value.as_str(), flag)),
                        });
                    }
                    "dns" | "sound" => send(MenuCommand::UpdateSetting {
                        key: action.to_string(),
                        value: value.to_string(),
                        revision: None,
                    }),
                    "copy" => preview::copy_proxy_command(ui),
                    "flush-dns" => {
                        #[cfg(windows)]
                        let _ = std::process::Command::new("ipconfig")
                            .arg("/flushdns")
                            .output();
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("/usr/bin/dscacheutil")
                            .arg("-flushcache")
                            .output();
                        toast("已刷新系统 DNS 缓存");
                    }
                    "stable" => {
                        STABLE_NODE.with(|s| {
                            if let Some((group, node)) = s.borrow().clone() {
                                send(MenuCommand::Select { group, node });
                            }
                        });
                    }
                    "update-id" => {
                        if let Ok(id) = value.parse() {
                            send(MenuCommand::UpdateSubscriptionId(id));
                        }
                    }
                    "use-id" => {
                        if let Ok(id) = value.parse() {
                            send(MenuCommand::UseSubscription(id));
                        }
                    }
                    "fix" => {
                        let state =
                            LIVE.with(|s| s.borrow().as_ref().unwrap().lock().unwrap().clone());
                        match (state.trouble.as_str(), value.as_str()) {
                            ("port", "secondary") => {
                                ui.set_page(5);
                                ui.invoke_switch_page(5);
                            }
                            ("sub", "primary") => {
                                ui.set_page(2);
                                ui.invoke_switch_page(2);
                            }
                            ("offline", "secondary") => send(MenuCommand::OpenNetworkSettings),
                            _ => send(MenuCommand::Recover(value.to_string())),
                        }
                    }
                    "edit" | "edit-id" => {
                        let edit = LIVE.with(|s| {
                            let binding = s.borrow();
                            let state = binding.as_ref().unwrap().lock().unwrap();
                            state
                                .preferences
                                .subscriptions
                                .iter()
                                .find(|sub| {
                                    if action == "edit-id" {
                                        Some(sub.id) == value.parse::<u64>().ok()
                                    } else {
                                        Some(&sub.url) == state.preferences.subscription.as_ref()
                                    }
                                })
                                .map(|s| (s.id, s.url.clone()))
                        });
                        if let Some((id, url)) = edit {
                            ui.set_editing_subscription(id.to_string().into());
                            ui.set_sub_editor_url(url.into());
                        }
                        ui.set_page(2);
                        toast("修改链接后点保存");
                    }
                    _ => toast("请在配置文件中编辑此项"),
                });
            }
        });
        let weak = ui.as_weak();
        ui.on_window_action(move |action| {
            if let Some(ui) = weak.upgrade() {
                match action.as_str() {
                    "minimize" => ui.window().set_minimized(true),
                    "maximize" => ui.window().set_maximized(!ui.window().is_maximized()),
                    "quit" if PREVIEW_MODE.with(Cell::get) => {
                        let _ = slint::quit_event_loop();
                    }
                    "quit" => send(MenuCommand::Quit),
                    _ if PREVIEW_MODE.with(Cell::get) => {
                        let _ = slint::quit_event_loop();
                    }
                    _ if !ui.get_close_to_tray() => {
                        send(MenuCommand::Quit);
                    }
                    _ => {
                        let _ = ui.hide();
                        send(MenuCommand::WindowClosed);
                        let _ = slint::invoke_from_event_loop(|| {
                            stop_motion();
                            reset_view();
                            WINDOW.with(|slot| {
                                slot.borrow_mut().take();
                            });
                            crate::status::release_unused_memory();
                        });
                    }
                }
            }
        });
        let weak = ui.as_weak();
        ui.on_drag_window(move |dx, dy| {
            if let Some(ui) = weak.upgrade() {
                let p = ui
                    .window()
                    .position()
                    .to_logical(ui.window().scale_factor());
                ui.window()
                    .set_position(slint::LogicalPosition::new(p.x + dx, p.y + dy));
            }
        });
        ui.on_toggle_run(|| send(MenuCommand::ToggleRun));
        ui.on_toggle_sys_proxy(|| {
            if PREVIEW_MODE.with(Cell::get) {
                preview::toggle_system_proxy();
            } else {
                send(MenuCommand::ToggleSystemProxy);
            }
        });
        ui.on_toggle_tun(|| {
            if PREVIEW_MODE.with(Cell::get) {
                with_ui(|ui| ui.set_tun_mode(!ui.get_tun_mode()));
            } else {
                send(MenuCommand::ToggleTun);
            }
        });
        ui.on_toggle_sound(|| {
            with_ui(|ui| {
                let next = !ui.get_sound_purr();
                ui.set_sound_purr(next);
                if next {
                    toast("连接后会听到呼噜");
                } else {
                    toast("已静音");
                }
            });
        });
        ui.on_set_mode(|mode| {
            let label = match mode.as_str() {
                "rule" => "规则分流",
                "global" => "全局代理",
                "direct" => "直连",
                _ => mode.as_str(),
            };
            toast(&format!("正在切换模式 → {label}"));
            send(MenuCommand::SetMode(mode.to_string()));
        });
        ui.on_select_group(|id| {
            SELECTED_GROUP.with(|slot| slot.set(id as usize));
            refresh_open();
            auto_test_selected_group();
        });
        ui.on_select_node(|group, node| {
            if node.is_empty() {
                return;
            }
            toast(&format!("{group} 正在换窝到 {node}"));
            send(MenuCommand::Select {
                group: group.to_string(),
                node: node.to_string(),
            });
        });
        ui.on_test_group(|group| {
            if !group.is_empty() {
                send(MenuCommand::TestGroup(group.to_string()));
            }
        });
        ui.on_close_connections(|| {
            send(MenuCommand::CloseConnections);
            toast("正在断开全部连接");
        });
        ui.on_close_connection(|id| {
            if PREVIEW_MODE.with(Cell::get) {
                LIVE.with(|l| {
                    let live = l.borrow().clone().unwrap();
                    let mut s = live.lock().unwrap();
                    s.connections.retain(|c| c.id != id.as_str());
                    s.bump();
                });
                refresh_open();
            } else {
                send(MenuCommand::CloseConnection(id.to_string()));
            }
        });
        ui.on_reload(|| {
            send(MenuCommand::Reload);
            toast("正在重载配置");
        });
        ui.on_clear_logs(clear_logs_local);
        ui.on_rule_query(|text| {
            RULE_QUERY.with(|query| *query.borrow_mut() = text.to_string());
            refresh_open();
        });
        ui.on_conn_query(|text| {
            CONN_QUERY.with(|query| *query.borrow_mut() = text.to_string());
            refresh_open();
        });
        ui.on_log_filter_changed(|value| {
            LOG_FILTER.with(|filter| *filter.borrow_mut() = value.to_string());
            refresh_open();
        });
        ui.on_switch_page(|page| {
            PAGE_STARTED.with(|t| t.set(Instant::now()));
            println!("purr: switch page to {page}");
            refresh_open();
            if page == 1 {
                auto_test_selected_group();
            }
            slint::Timer::single_shot(
                Duration::from_millis(100),
                crate::status::release_unused_memory,
            );
        });
        ui.window().on_close_requested(|| {
            println!("purr: window closed from OS / taskbar");
            send(MenuCommand::Quit);
            slint::CloseRequestResponse::HideWindow
        });
        design::wire(&ui);
        if let Err(err) = ui.show() {
            eprintln!("purr: show window failed: {err}");
            return false;
        }
        #[cfg(windows)]
        {
            fit_to_work_area(&ui);
            // winit may not have a native window yet on the first call, and Windows places a new
            // window at its cascade default after show; repeat once the event loop has run.
            slint::Timer::single_shot(Duration::from_millis(80), || {
                with_ui(fit_to_work_area);
                with_ui(apply_frame_shape);
            });
        }
        ui.window().with_winit_window(|w| {
            #[cfg(target_os = "macos")]
            {
                use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(handle) = w.window_handle() {
                    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
                        unsafe {
                            let view = handle.ns_view.cast::<objc2::runtime::AnyObject>().as_ref();
                            let layer: *mut objc2::runtime::AnyObject = objc2::msg_send![view, layer];
                            if !layer.is_null() {
                                let _: () = objc2::msg_send![layer, setCornerRadius: 12.0f64];
                                let _: () = objc2::msg_send![layer, setMasksToBounds: objc2::runtime::Bool::YES];
                            }
                        }
                    }
                }
            }
            #[cfg(windows)]
            {
                use std::ffi::c_void;
                use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                #[link(name = "dwmapi")]
                unsafe extern "system" {
                    fn DwmSetWindowAttribute(
                        hwnd: isize,
                        dw_attribute: u32,
                        pv_attribute: *const c_void,
                        cb_attribute: u32,
                    ) -> i32;
                }
                #[link(name = "kernel32")]
                unsafe extern "system" {
                    fn GetModuleHandleW(lp_module_name: *const u16) -> isize;
                }
                #[link(name = "user32")]
                unsafe extern "system" {
                    fn LoadImageW(
                        hinst: isize,
                        name: *const u16,
                        type_: u32,
                        cx: i32,
                        cy: i32,
                        fu_load: u32,
                    ) -> isize;
                    fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
                }
                if let Ok(handle) = w.window_handle() {
                    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                        unsafe {
                            let hwnd = handle.hwnd.get();
                            let corner_pref: u32 = 2; // DWMWCP_ROUND
                            let _ = DwmSetWindowAttribute(
                                hwnd,
                                33, // DWMWA_WINDOW_CORNER_PREFERENCE
                                (&corner_pref as *const u32).cast::<c_void>(),
                                std::mem::size_of::<u32>() as u32,
                            );
                            let hinst = GetModuleHandleW(std::ptr::null());
                            // MAKEINTRESOURCEW(1): resource id 1, not a real pointer.
                            #[allow(clippy::manual_dangling_ptr)]
                            let icon_id = 1 as *const u16;
                            let hicon_big = LoadImageW(hinst, icon_id, 1, 64, 64, 0);
                            let hicon_small = LoadImageW(hinst, icon_id, 1, 16, 16, 0);
                            if hicon_big != 0 {
                                SendMessageW(hwnd, 0x0080, 1, hicon_big); // WM_SETICON, ICON_BIG
                            }
                            if hicon_small != 0 {
                                SendMessageW(hwnd, 0x0080, 0, hicon_small); // WM_SETICON, ICON_SMALL
                            }
                        }
                    }
                }
            }
        });
        println!("purr: window open");
        *slot.borrow_mut() = Some(ui);
        SEEN.with(|seen| seen.set(u64::MAX));
        true
    });
    if !created {
        return;
    }
    reset_view();
    refresh_open();
    start_motion();
    slint::Timer::single_shot(
        Duration::from_millis(100),
        crate::status::release_unused_memory,
    );
}

pub fn finish_subscription_edit(id: u64) {
    with_ui(|ui| {
        if ui.get_editing_subscription().parse::<u64>().ok() == Some(id) {
            ui.set_editing_subscription("".into());
            ui.set_sub_editor_url("".into());
        }
    });
}

fn toast(msg: &str) {
    with_ui(|ui| {
        ui.set_toast_msg(msg.into());
        ui.set_toast_on(true);
    });
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(1600),
        move || {
            with_ui(|ui| ui.set_toast_on(false));
        },
    );
    TOAST_TIMER.with(|slot| *slot.borrow_mut() = Some(timer));
}

fn reset_view() {
    RULE_QUERY.with(|query| query.borrow_mut().clear());
    CONN_QUERY.with(|query| query.borrow_mut().clear());
    LOG_FILTER.with(|filter| *filter.borrow_mut() = "all".to_string());
    SPECTRUM_CACHE.with(|slot| *slot.borrow_mut() = None);
    LIST_CACHE.with(|slot| *slot.borrow_mut() = None);
    GROUP_TABS_CACHE.with(|slot| *slot.borrow_mut() = None);
    CONNS.with(|slot| *slot.borrow_mut() = None);
    RULES.with(|slot| *slot.borrow_mut() = None);
    RULE_INPUT.with(|slot| *slot.borrow_mut() = None);
    LOGS.with(|slot| *slot.borrow_mut() = None);
    SUBSCRIPTIONS.with(|slot| *slot.borrow_mut() = None);
    STABLE_NODE.with(|slot| *slot.borrow_mut() = None);
    LAST_NOTICE.with(|s| s.set(0));
    LAST_ERROR.with(|s| s.borrow_mut().clear());
    LAST_TICK.with(|tick| tick.set(None));
    HOLD_ARMED.with(|armed| armed.set(true));
}

fn with_ui(apply_ui: impl FnOnce(&MainWindow)) {
    WINDOW.with(|slot| {
        let binding = slot.borrow();
        if let Some(ui) = binding.as_ref() {
            apply_ui(ui);
        }
    });
}
pub fn verify_tab(page: i32) {
    with_ui(|ui| {
        use slint::platform::{PointerEventButton, WindowEvent};
        let width = ui
            .window()
            .size()
            .to_logical(ui.window().scale_factor())
            .width;
        let position = slint::LogicalPosition::new(width * (page as f32 + 0.5) / 6.0, 164.0);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        assert_eq!(
            ui.get_page(),
            page,
            "native tab event did not select requested page"
        );
    });
}

fn begin_setting(key: &str, value: bool) -> u64 {
    let revision = SETTING_REVISION.with(|counter| {
        let next = counter.get() + 1;
        counter.set(next);
        next
    });
    PENDING_SETTINGS.with(|pending| pending.borrow_mut().insert(key.into(), (revision, value)));
    revision
}

fn setting_value(key: &str, committed: bool) -> bool {
    PENDING_SETTINGS.with(|pending| {
        pending
            .borrow()
            .get(key)
            .map_or(committed, |(_, value)| *value)
    })
}

pub fn finish_setting(key: &str, revision: u64) {
    PENDING_SETTINGS.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.get(key).is_some_and(|(id, _)| *id == revision) {
            pending.remove(key);
        }
    });
    refresh_open();
}

fn refresh_open() {
    let Some(live) = LIVE.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let state = live.lock().expect("live lock").clone();
    SEEN.with(|seen| seen.set(state.generation));
    with_ui(|ui| apply(ui, &state));
    TRAY_WINDOW.with(|s| {
        if let Some(ui) = s.borrow().as_ref() {
            apply_tray(ui, &state);
        }
    });
}

fn start_motion() {
    let started = Instant::now();
    let mut ex = 0.0f32;
    let mut ev = 0.0f32;
    let mut cur_page = -1i32;
    let mut prev_page = -1i32;
    let mut page_changed = Instant::now() - Duration::from_secs(5);
    let mut ear_twitch = 0.0f32;
    let mut ear_random = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64;
    let mut last_plot = (Vec::<(u64, u64)>::new(), 0.0f32, 0.0f32, false);
    let mut last_clock_ms = -1000.0f32;
    let mut last_wave_ms = -1000.0f32;
    let mut last_chart_ms = -1000.0f32;
    let mut last_ear_x = -1.0f32;
    let mut last_awake = false;
    let mut last_hold = -1.0f32;
    let mut last_trim_ms = 0.0f32;
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(32),
        move || {
            let now = Instant::now();
            let dt = LAST_TICK
                .with(|tick| tick.get())
                .map(|then| now.saturating_duration_since(then).as_secs_f32())
                .unwrap_or(0.032)
                .clamp(0.0, 0.08);
            LAST_TICK.with(|tick| tick.set(Some(now)));
            let millis = started.elapsed().as_secs_f32() * 1000.0;
            with_ui(|ui| {
                if !ui.window().is_visible() {
                    return;
                }
                let (down, _flow, history) = LIVE
                    .with(|slot| {
                        slot.borrow().as_ref().map(|live| {
                            let guard = live.lock().expect("live lock");
                            (guard.down_bps, guard.flow, guard.history.clone())
                        })
                    })
                    .unwrap_or((0, 0.0, Vec::new()));
                let awake = ui.get_awake();
                let reduced = ui.get_reduced_motion();
                step_hold(ui, dt);
                if ui.global::<IconClock>().get_awake() != awake {
                    ui.global::<IconClock>().set_awake(awake);
                }
                if ui.global::<IconClock>().get_reduced() != reduced {
                    ui.global::<IconClock>().set_reduced(reduced);
                }
                // macOS switches pages without a transition.
                let next_page_p = 1.0;
                if (ui.get_page_progress() - next_page_p).abs() > 0.001 {
                    ui.set_page_progress(next_page_p);
                }
                let next_shake = if ui.get_first_focus() && !reduced {
                    // Import nudge: three quick 6 px pushes to the left in 0.3 s, ending at rest.
                    let t = SHAKE_STARTED.with(|s| s.get().elapsed().as_secs_f32());
                    if t < 0.3 {
                        -6.0 * (3.0 * std::f32::consts::PI * t / 0.3).sin().abs()
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                if (ui.get_first_shake() - next_shake).abs() > 0.01 {
                    ui.set_first_shake(next_shake);
                }
                {
                    let kind = if ui.global::<S>().get_first() {
                        "empty".to_string()
                    } else {
                        ui.global::<S>().get_err().to_string()
                    };
                    if !kind.is_empty() && ui.get_page() == 0 {
                        let t = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64();
                        let t = if reduced {
                            0.0
                        } else {
                            (t / 0.35).floor() * 0.35
                        };
                        let key = format!("{kind}:{t:.2}");
                        if ASCII_KEY.with(|k| *k.borrow() != key) {
                            ASCII_KEY.with(|k| *k.borrow_mut() = key);
                            let rows = present::ascii_cat(
                                &kind,
                                ((t / 0.45) as i64) % 2 == 1,
                                (t * 2.8).sin() * 0.06,
                            )
                            .into_iter()
                            .map(|(row, alpha, text)| AsciiRow {
                                row,
                                alpha,
                                text: text.into(),
                            })
                            .collect::<Vec<_>>();
                            ui.global::<S>()
                                .set_ascii(ModelRc::new(VecModel::from(rows)));
                        }
                    }
                }
                let progress = ui.get_flood_progress();
                if progress > 0.0 && progress < 1.0 {
                    ui.set_flood_path(
                        present::flood_path(
                            ui.get_line_w(),
                            ui.window()
                                .size()
                                .to_logical(ui.window().scale_factor())
                                .height,
                            ui.global::<S>().get_flood_x(),
                            progress,
                        )
                        .into(),
                    );
                }
                let load = (down as f32 / (16.0 * 1024.0 * 1024.0)).clamp(0.0, 1.0);
                let next_amp = 1.5 + (down as f32 / (16.0 * 1048576.0)).clamp(0.0, 1.0) * 4.0;
                if (ui.global::<S>().get_amp() - next_amp).abs() > 0.02 {
                    ui.global::<S>().set_amp(next_amp);
                }
                let elapsed = if awake {
                    elapsed_text()
                } else {
                    LIVE.with(|l| {
                        l.borrow()
                            .as_ref()
                            .map(|s| present::format_hms(s.lock().unwrap().last_nap))
                            .unwrap_or_default()
                    })
                };
                if ui.get_elapsed().as_str() != elapsed {
                    ui.set_elapsed(elapsed.into());
                }
                if millis - last_trim_ms >= 20_000.0 {
                    last_trim_ms = millis;
                    crate::status::release_unused_memory();
                }

                // PurrStore.currentEarX: closed-form damped spring from the previous page's
                // column, 0.55 s, x = start + (target - start) * (1 - e^(-9.5t) cos 14t).
                let line_w = ui.get_line_w();
                let page = ui.get_page();
                if page != cur_page {
                    prev_page = if cur_page < 0 { page } else { cur_page };
                    cur_page = page;
                    page_changed = Instant::now();
                }
                let column = |p: i32| {
                    if line_w > 0.0 {
                        line_w * (p as f32 + 0.5) / 6.0
                    } else {
                        100.0
                    }
                };
                let target_x = column(page);
                let start_x = column(prev_page);
                let since = page_changed.elapsed().as_secs_f32();
                let (next_x, speed) = if reduced || since >= 0.55 || prev_page == page {
                    (target_x, 0.0)
                } else {
                    let progress = 1.0 - (-9.5 * since).exp() * (14.0 * since).cos();
                    let velocity = (target_x - start_x).abs() * (-9.5 * since).exp();
                    (
                        start_x + (target_x - start_x) * progress,
                        (velocity / 260.0).min(1.0),
                    )
                };
                ex = next_x;
                ev = if speed > 0.0 { speed } else { 0.0 };
                if (ui.get_ear_x() - ex).abs() > 0.05 {
                    ui.set_ear_x(ex);
                }
                if page == 3 && ui.get_conn_pane() == 1 {
                    let clock = present::clock_now();
                    if ui.global::<S>().get_log_now().as_str() != clock {
                        ui.global::<S>().set_log_now(clock.into());
                    }
                }

                let holding = ui.get_holding();
                let hold = if holding { ui.get_hold() } else { 0.0 };
                ear_random = ear_random.wrapping_mul(6364136223846793005).wrapping_add(1);
                // macOS has no idle ear twitch.
                ear_twitch *= 0.85;
                if reduced || ear_twitch < 0.02 {
                    ear_twitch = 0.0;
                }

                let ear_moved = (ex - last_ear_x).abs() > 0.01
                    || awake != last_awake
                    || (hold - last_hold).abs() > 0.005;
                let active_motion = holding
                    || next_page_p < 1.0
                    || ui.get_first_focus()
                    || (progress > 0.0 && progress < 1.0)
                    || ev.abs() > 0.0
                    || ear_twitch > 0.0
                    || ear_moved;

                let clock_interval = if active_motion {
                    30.0
                } else if awake {
                    80.0
                } else if !ui.get_first_run() && page == 0 {
                    150.0
                } else {
                    1000.0
                };
                if !reduced && (millis - last_clock_ms) >= clock_interval {
                    last_clock_ms = millis;
                    ui.set_frame_time(millis);
                    ui.global::<IconClock>().set_time(millis);
                }

                let wave_interval = if active_motion {
                    30.0
                } else if awake {
                    80.0
                } else {
                    0.0
                };
                if active_motion
                    || (awake && !reduced && (millis - last_wave_ms) >= wave_interval)
                    || ui.get_line_path().is_empty()
                {
                    last_wave_ms = millis;
                    last_ear_x = ex;
                    last_awake = awake;
                    last_hold = hold;
                    let line = present::ear_wave_v5(
                        line_w,
                        ui.get_line_h().max(136.0),
                        ex,
                        ev,
                        if awake || hold > 0.0 { millis } else { 0.0 },
                        awake,
                        hold,
                        load,
                        ui.get_mode().as_str(),
                        !ui.get_trouble().is_empty(),
                        reduced,
                        ear_twitch,
                        awake && !ui.get_sys_proxy() && !ui.get_tun_mode(),
                    );
                    if ui.get_line_path().as_str() != line.main {
                        ui.set_line_path(line.main.into());
                    }
                    if ui.get_line_warm().as_str() != line.warm {
                        ui.set_line_warm(line.warm.into());
                    }
                }

                let chart_phase =
                    (awake && !reduced && ui.get_mode() != "direct").then_some(millis);
                let chart_changed = chart_phase.is_some() != last_plot.3
                    || history != last_plot.0
                    || ui.get_chart_w() != last_plot.1
                    || ui.get_chart_h() != last_plot.2;
                if page == 0
                    && (chart_changed
                        || (chart_phase.is_some() && (millis - last_chart_ms) >= 200.0))
                {
                    last_chart_ms = millis;
                    let plot =
                        present::chart(&history, ui.get_chart_w(), ui.get_chart_h(), chart_phase);
                    if ui.get_chart_fill().as_str() != plot.fill {
                        ui.set_chart_fill(plot.fill.into());
                    }
                    if ui.get_chart_down().as_str() != plot.down {
                        ui.set_chart_down(plot.down.into());
                    }
                    if ui.get_chart_up().as_str() != plot.up {
                        ui.set_chart_up(plot.up.into());
                    }
                    if ui.get_chart_grid().as_str() != plot.grid {
                        ui.set_chart_grid(plot.grid.into());
                    }
                    let peak_str = format!("PEAK {}", plot.peak.to_uppercase());
                    if ui.get_peak_text().as_str() != peak_str {
                        ui.set_peak_text(peak_str.into());
                    }
                    let max = history.iter().map(|(_, d)| *d).max().unwrap_or(0) as f64 / 1048576.0;
                    let max = (max / 5.0).ceil().max(1.0) * 5.0;
                    let max_str = format!("{max:.0}");
                    if ui.get_chart_max().as_str() != max_str {
                        ui.set_chart_max(max_str.into());
                    }
                    let mid_str = format!("{}", (max / 2.0).floor() as i64);
                    if ui.get_chart_mid().as_str() != mid_str {
                        ui.set_chart_mid(mid_str.into());
                    }
                    last_plot = (
                        history.clone(),
                        ui.get_chart_w(),
                        ui.get_chart_h(),
                        chart_phase.is_some(),
                    );
                }
            });
        },
    );
    MOTION.with(|slot| *slot.borrow_mut() = Some(timer));
}

/// Keep the first window inside the monitor's work area: at 125 % on a 1536 px screen the
/// 1200 × 760 window is wider/taller than the usable area. Shrink to fit (down to the
/// window's minimum) and centre it on the monitor the OS chose.
#[cfg(windows)]
fn fit_to_work_area(ui: &MainWindow) {
    use std::ffi::c_void;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    #[allow(clippy::upper_case_acronyms)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    #[allow(clippy::upper_case_acronyms)]
    struct MONITORINFO {
        cb_size: u32,
        rc_monitor: RECT,
        rc_work: RECT,
        dw_flags: u32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MonitorFromWindow(hwnd: isize, dw_flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(h_monitor: *mut c_void, lpmi: *mut MONITORINFO) -> i32;
    }
    let placed = ui.window().with_winit_window(|w| {
        let Ok(handle) = w.window_handle() else {
            return None;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        let hwnd = handle.hwnd.get();
        let mut mi = MONITORINFO {
            cb_size: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let ok = unsafe {
            let hmon = MonitorFromWindow(hwnd, 2 /* MONITOR_DEFAULTTONEAREST */);
            !hmon.is_null() && GetMonitorInfoW(hmon, &mut mi) != 0
        };
        if !ok {
            return None;
        }
        let work = mi.rc_work;
        let scale = w.scale_factor().max(1.0);
        let (work_w, work_h) = (
            (work.right - work.left) as f64,
            (work.bottom - work.top) as f64,
        );
        let margin = 12.0 * scale;
        let fit =
            |want: f64, min: f64, avail: f64| want.min(avail - 2.0 * margin).max(min.min(avail));
        let width = fit(1200.0 * scale, 1000.0 * scale, work_w).round();
        let height = fit(760.0 * scale, 560.0 * scale, work_h).round();
        let left = work.left + ((work_w - width) / 2.0).round() as i32;
        let top = work.top + ((work_h - height) / 2.0).round().max(0.0) as i32;
        let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(width as u32, height as u32));
        w.set_outer_position(winit::dpi::PhysicalPosition::new(left, top));
        Some((width as u32, height as u32, left, top))
    });
    // Mirror through Slint's own window API so its cached geometry agrees with the native window.
    if let Some(Some((width, height, left, top))) = placed {
        ui.window()
            .set_size(slint::PhysicalSize::new(width, height));
        ui.window()
            .set_position(slint::PhysicalPosition::new(left, top));
    }
}

/// Windows 11 build 22000 or later (DWM rounds corners on request).
#[cfg(windows)]
fn is_windows_11() -> bool {
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }
    thread_local! {
        static WIN11: Cell<Option<bool>> = const { Cell::new(None) };
    }
    WIN11.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = {
            let mut info = OsVersionInfoW {
                size: std::mem::size_of::<OsVersionInfoW>() as u32,
                major: 0,
                minor: 0,
                build: 0,
                platform: 0,
                csd: [0; 128],
            };
            unsafe { RtlGetVersion(&mut info) == 0 && info.major >= 10 && info.build >= 22000 }
        };
        c.set(Some(v));
        v
    })
}

/// Round the borderless main window like the macOS one. Windows 11: DWM clips the corners
/// (anti-aliased, with the system shadow; system radius 8 px) and the drawn hairline follows
/// that radius. Windows 10: a window region with the macOS 12 px radius scaled by DPI.
/// Maximized: square corners, no region.
#[cfg(windows)]
fn apply_frame_shape(ui: &MainWindow) {
    use std::ffi::c_void;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    #[repr(C)]
    #[derive(Default)]
    #[allow(clippy::upper_case_acronyms)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, pv: *const c_void, cb: u32) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd: isize, rect: *mut RECT) -> i32;
        fn SetWindowRgn(hwnd: isize, hrgn: isize, redraw: i32) -> i32;
        fn IsZoomed(hwnd: isize) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> isize;
    }
    let win11 = is_windows_11();
    let shaped = ui.window().with_winit_window(|w| {
        let Ok(handle) = w.window_handle() else {
            return None;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        let hwnd = handle.hwnd.get();
        let maximized = w.is_maximized() || unsafe { IsZoomed(hwnd) } != 0;
        let scale = w.scale_factor().max(1.0);
        unsafe {
            if win11 {
                let pref: u32 = if maximized {
                    1 /* DWMWCP_DONOTROUND */
                } else {
                    2 /* DWMWCP_ROUND */
                };
                let _ = DwmSetWindowAttribute(hwnd, 33, (&pref as *const u32).cast(), 4);
                // The drawn hairline is the border, as on macOS; no second system border.
                let none: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
                let _ = DwmSetWindowAttribute(hwnd, 34, (&none as *const u32).cast(), 4);
            } else if maximized {
                let _ = SetWindowRgn(hwnd, 0, 1);
            } else {
                let mut r = RECT::default();
                if GetWindowRect(hwnd, &mut r) != 0 {
                    let d = (2.0 * 12.0 * scale).round() as i32;
                    let rgn =
                        CreateRoundRectRgn(0, 0, r.right - r.left + 1, r.bottom - r.top + 1, d, d);
                    if rgn != 0 {
                        // The system owns the region after this call.
                        let _ = SetWindowRgn(hwnd, rgn, 1);
                    }
                }
            }
        }
        Some(maximized)
    });
    if let Some(Some(maximized)) = shaped {
        ui.set_frame_radius(if maximized {
            0.0
        } else if win11 {
            8.0
        } else {
            12.0
        });
    }
}

fn step_hold(ui: &MainWindow, dt: f32) {
    if ui.get_holding() {
        if ui.get_first_run() {
            ui.set_holding(false);
            ui.set_hold(0.0);
            ui.set_page(0);
            ui.invoke_switch_page(0);
            ui.set_first_focus(true);
            SHAKE_STARTED.with(|s| s.set(Instant::now()));
            slint::Timer::single_shot(Duration::from_millis(400), || {
                with_ui(|ui| ui.set_first_focus(false))
            });
            toast("先给它一个窝 · 导入订阅");
            return;
        }
        if HOLD_ARMED.with(|armed| armed.get()) {
            let hold = (ui.get_hold() + dt / if ui.get_awake() { 1.6 } else { 1.0 }).min(1.0);
            ui.set_hold(hold);
            ui.set_tip_text(if ui.get_awake() {
                "继续按住…哄它睡觉".into()
            } else {
                "继续按住…它在醒了".into()
            });
            if hold >= 1.0 {
                HOLD_ARMED.with(|armed| armed.set(false));
                ui.set_holding(false);
                ui.set_hold(0.0);
                send(MenuCommand::ToggleRun);
            }
        }
    } else {
        HOLD_ARMED.with(|armed| armed.set(true));
        ui.set_tip_text(if ui.get_first_run() {
            "先导入订阅，再按住耳朵".into()
        } else if ui.get_awake() {
            "按住耳朵 / Space 哄它睡觉".into()
        } else {
            "按住耳朵 / Space 叫醒它".into()
        });
    }
}

fn stop_motion() {
    MOTION.with(|slot| slot.borrow_mut().take());
}

pub fn sync(live: &Arc<Mutex<LiveState>>) {
    if WINDOW.with(|s| s.borrow().is_none()) && TRAY_WINDOW.with(|s| s.borrow().is_none()) {
        return;
    }
    let state = {
        let guard = live.lock().expect("live lock");
        let seen = SEEN.with(|seen| seen.get());
        if guard.generation == seen {
            return;
        }
        SEEN.with(|seen| seen.set(guard.generation));
        guard.clone()
    };
    with_ui(|ui| apply(ui, &state));
    TRAY_WINDOW.with(|s| {
        if let Some(ui) = s.borrow().as_ref() {
            apply_tray(ui, &state);
        }
    });
}

fn apply(ui: &MainWindow, state: &LiveState) {
    apply_sites(ui, state);
    ui.set_tray_nodes(ModelRc::new(VecModel::from(tray_nodes(state))));
    let tun_auth = crate::tray::tun_authorized();
    if ui.global::<S>().get_tun_auth() != tun_auth {
        ui.global::<S>().set_tun_auth(tun_auth);
    }
    ui.set_awake(state.awake);
    let (down_value, down_unit) = if state.traffic_live {
        present::rate_parts(state.down_bps)
    } else {
        ("—".into(), String::new())
    };
    let (up_value, up_unit) = if state.traffic_live {
        present::rate_parts(state.up_bps)
    } else {
        ("—".into(), String::new())
    };
    ui.set_down_value(down_value.into());
    ui.set_down_unit(down_unit.into());
    ui.set_up_value(up_value.into());
    ui.set_up_unit(up_unit.into());
    ui.set_mode(state.mode.as_str().into());
    ui.set_config_path(state.config_dir.as_str().into());
    ui.set_mixed_port(format!("{}", state.preferences.mixed_port).into());
    ui.set_sound_choice(state.preferences.sound.as_str().into());
    ui.set_dns_choice(state.preferences.dns.as_str().into());
    ui.set_sys_proxy(state.preferences.system_proxy);
    ui.set_tun_mode(state.tun);
    ui.set_auto_wake(setting_value("auto-wake", state.preferences.auto_wake));
    ui.set_auto_swap(setting_value("auto-swap", state.preferences.auto_swap));
    ui.set_block_ads(setting_value("block-ads", state.preferences.block_ads));
    ui.set_close_to_tray(setting_value(
        "close-to-tray",
        state.preferences.close_to_tray,
    ));
    ui.set_allow_lan(setting_value("allow-lan", state.preferences.allow_lan));
    ui.set_ipv6(setting_value("ipv6", state.preferences.ipv6));
    ui.set_busy(state.busy);
    let total_memory = state.memory_shell + state.memory_core;
    ui.set_memory_value(format!("{:.1}", total_memory as f64 / 1_000_000.0).into());
    ui.set_memory_fraction((total_memory as f32 / 150_000_000.0).min(1.0));
    ui.set_memory_detail(
        format!(
            "主程序 {} · 内核 {} MB",
            state.memory_shell / 1_000_000,
            state.memory_core / 1_000_000
        )
        .into(),
    );
    if !PREVIEW_MODE.with(Cell::get) {
        let was_first = ui.get_first_run();
        ui.set_first_run(
            state
                .groups
                .iter()
                .all(|g| g.nodes.iter().all(|n| present::skip_delay(&n.kind)))
                && state.preferences.subscription.is_none(),
        );
        if was_first && !ui.get_first_run() {
            ui.set_page(0);
            PAGE_STARTED.with(|time| time.set(Instant::now()));
            ui.global::<S>().set_tip_nudge(true);
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(1500), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<S>().set_tip_nudge(false);
                }
            });
        }
    }
    ui.set_sub_name(
        if state.preferences.subscription.is_some() {
            "已导入订阅"
        } else {
            "本地配置"
        }
        .into(),
    );
    ui.set_has_subscription(state.preferences.subscription.is_some());
    ui.set_total_value(
        state
            .session_down
            .split_whitespace()
            .next()
            .unwrap_or("0")
            .into(),
    );
    ui.set_total_unit(
        state
            .session_down
            .split_whitespace()
            .nth(1)
            .unwrap_or("B")
            .into(),
    );
    ui.set_total_detail(format!("↓ {} · ↑ {}", state.session_down, state.session_up).into());
    ui.set_conn_detail(format!("当前 {} 条连接", state.connections.len()).into());
    ui.set_connection_count(state.connections.len() as i32);
    ui.set_proxy_detail(present::size_text(state.route_down[0]).into());
    ui.set_direct_detail(present::size_text(state.route_down[1]).into());
    ui.set_blocked_detail(state.blocked_connections.to_string().into());
    let routed = state.route_down.iter().sum::<u64>();
    ui.set_proxy_fraction(if routed > 0 {
        state.route_down[0] as f32 / routed as f32
    } else {
        0.0
    });
    let (ip, location) = state.place.split_once(" · ").unwrap_or(("—", &state.place));
    ui.set_exit_ip(ip.into());
    ui.set_exit_location(location.into());
    LAST_ERROR.with(|s| {
        if *s.borrow() != state.last_error {
            *s.borrow_mut() = state.last_error.clone();
            if !state.last_error.is_empty() {
                toast(&state.last_error);
            }
        }
    });
    LAST_NOTICE.with(|s| {
        if s.get() != state.notice_id {
            s.set(state.notice_id);
            if !state.notice.is_empty() {
                toast(&state.notice);
            }
        }
    });

    let current = current_node(state);
    ui.set_current_group_name(current.group.as_str().into());
    ui.set_current_node_label(current.label.as_str().into());
    ui.set_current_node_delay(current.delay.as_str().into());

    if state.awake {
        ui.set_status_header_text(
            format!("STATUS 呼噜中 · {} · {}", current.label, current.delay).into(),
        );
        ui.set_now_purring_name(current.label.as_str().into());
        let desc = match state.mode.as_str() {
            "global" => format!("全局：所有流量都经由 {}。", current.label),
            "direct" => "直连：醒着，但不带路，流量全部直走。".into(),
            _ => format!("分流：国内直走，其余经由 {}。", current.label),
        };
        ui.set_now_purring_desc(
            format!(
                "{desc}{}",
                if !state.preferences.system_proxy && !state.tun {
                    "系统代理和 TUN 都关着，还没有流量经过它。"
                } else {
                    ""
                }
            )
            .into(),
        );
    } else {
        ui.set_status_header_text("STATUS 睡着了 · OFF".into());
        ui.set_now_purring_name("未连接".into());
        let mode = match state.mode.as_str() {
            "global" => "全局",
            "direct" => "直连",
            _ => "分流",
        };
        ui.set_now_purring_desc(
            format!(
                "睡着了：系统代理已还原，内核仍在 {} 端口待命。醒来后按 {mode} 走。",
                state.preferences.mixed_port
            )
            .into(),
        );
    }
    if !PREVIEW_MODE.with(Cell::get) {
        apply_subscriptions(ui, state);
        apply_trouble(ui, state);
        let total = present::size_text(state.telemetry.down.saturating_add(state.telemetry.up));
        let (value, unit) = total.split_once(' ').unwrap_or(("0", "B"));
        ui.set_total_value(value.into());
        ui.set_total_unit(unit.into());
        ui.set_total_detail(
            format!(
                "↓ {} · ↑ {}",
                present::size_text(state.telemetry.down),
                present::size_text(state.telemetry.up)
            )
            .into(),
        );
        ui.set_proxy_detail(present::size_text(state.telemetry.routes[0]).into());
        ui.set_direct_detail(present::size_text(state.telemetry.routes[1]).into());
        ui.set_blocked_detail(state.telemetry.blocked.to_string().into());
        let total = state.telemetry.routes.iter().sum::<u64>();
        ui.set_proxy_fraction(if total > 0 {
            state.telemetry.routes[0] as f32 / total as f32
        } else {
            0.
        });
    }

    publish_spectrum(ui, state);
    publish_conns(ui, state);
    publish_rules(ui, state);
    publish_logs(ui, state);
    if PREVIEW_MODE.with(Cell::get) {
        preview::decorate(ui, state);
    } else {
        inspect_live(ui, state, ui.get_hover_node());
    }
}
fn apply_subscriptions(ui: &MainWindow, state: &LiveState) {
    let now = crate::subscriptions::now();
    let rows = state
        .preferences
        .subscriptions
        .iter()
        .map(|s| {
            let age = now.saturating_sub(s.updated);
            let when = if s.updated == 0 {
                "尚未更新".into()
            } else if age < 60 {
                "刚刚更新".into()
            } else if age < 3600 {
                format!("更新于 {} 分钟前", age / 60)
            } else if age < 86400 {
                format!("更新于 {} 小时前", age / 3600)
            } else {
                format!("更新于 {} 天前", age / 86400)
            };
            SubscriptionRow {
                id: s.id.to_string().into(),
                title: s.name.as_str().into(),
                detail: format!("{} 个窝点 · {when}", s.nodes).into(),
                used: s
                    .used()
                    // macOS shows whole GB (integer division in purr_core_snapshot_json).
                    .map(|n| (n / 1073741824).to_string())
                    .unwrap_or_else(|| "—".into())
                    .into(),
                total: s
                    .total
                    .map(|n| (n / 1073741824).max(1).to_string())
                    .unwrap_or_else(|| "—".into())
                    .into(),
                fraction: s
                    .used()
                    .zip(s.total)
                    .filter(|(_, t)| *t > 0)
                    .map(|(u, t)| (u as f32 / t as f32).min(1.))
                    .unwrap_or(0.),
                expiry: s
                    .expires
                    .filter(|t| *t > 0)
                    .map(crate::subscriptions::date)
                    .unwrap_or_else(|| "未提供".into())
                    .into(),
                active: Some(&s.url) == state.preferences.subscription.as_ref(),
                expired: s.exhausted(),
            }
        })
        .collect::<Vec<_>>();
    publish(&SUBSCRIPTIONS, rows, |model| ui.set_subscriptions(model));
    if let Some(s) = state
        .preferences
        .subscriptions
        .iter()
        .find(|s| Some(&s.url) == state.preferences.subscription.as_ref())
    {
        ui.set_sub_name(s.name.as_str().into());
        let remaining = s
            .total
            .zip(s.used())
            .map(|(t, u)| format!("剩 {}", present::size_text(t.saturating_sub(u))))
            .unwrap_or_else(|| "额度未提供".into());
        let expires = s
            .expires
            .filter(|t| *t > 0)
            .map(|t| {
                if t <= now {
                    "已到期".into()
                } else {
                    format!("{} 天后到期", (t - now) / 86400)
                }
            })
            .unwrap_or_else(|| "到期时间未提供".into());
        ui.set_sub_detail(format!("{remaining} · {expires}").into());
    }
}
fn apply_trouble(ui: &MainWindow, state: &LiveState) {
    ui.set_trouble(state.trouble.as_str().into());
    let (title, description, action, secondary) = trouble_text(state);
    ui.set_trouble_title(title.into());
    ui.set_trouble_desc(description.into());
    ui.set_trouble_action(action.into());
    ui.set_trouble_secondary(secondary.into());
    ui.set_trouble_countdown(countdown(state).into());
    if !state.trouble.is_empty() {
        ui.set_status_header_text(format!("STATUS 卡住了 · {}", ui.get_trouble_title()).into());
    }
}
fn countdown(state: &LiveState) -> String {
    state
        .swap_at
        .map(|t| {
            format!(
                "换个窝继续睡 已开启 · {} 秒后自动换",
                t.saturating_duration_since(Instant::now()).as_secs() + 1
            )
        })
        .unwrap_or_default()
}
fn trouble_text(state: &LiveState) -> (String, String, String, &'static str) {
    let node = current_node(state);
    match state.trouble.as_str() {
        "timeout" => (
            format!("{} 没回应了", node.label),
            "连续 3 次测速没有回音。可以换个窝继续，或再试一次。".into(),
            "换到可用的窝点".into(),
            "再试一次",
        ),
        "offline" => (
            "电脑自己断网了".into(),
            "当前没有可用的网络连接，跟窝点无关。连上网之后它会自己继续呼噜。".into(),
            "重新检测".into(),
            "打开网络设置",
        ),
        "sub" => (
            format!(
                "{} 到期或额度用尽",
                state
                    .preferences
                    .subscriptions
                    .iter()
                    .find(|s| Some(&s.url) == state.preferences.subscription.as_ref())
                    .map(|s| s.name.as_str())
                    .unwrap_or("当前订阅")
            ),
            "更新订阅信息，或先换用可用的备用订阅。".into(),
            "去订阅 / 更新".into(),
            "换用备用订阅",
        ),
        "port" => (
            format!("{} 被别人占了", state.trouble_port),
            "这个端口已有程序在监听。可以自动找一个空闲端口，或到习惯里修改。".into(),
            "换一个空闲端口".into(),
            "去习惯里改",
        ),
        "core" => (
            "代理内核没有回应".into(),
            "正在等待内核恢复，也可以重新检测配置。".into(),
            "重新检测".into(),
            "再试一次",
        ),
        _ => (String::new(), String::new(), String::new(), "再试一次"),
    }
}

struct Current {
    group: String,
    label: String,
    delay: String,
}

/// The group the tray and the overview act on. Global mode: GLOBAL. Otherwise the top-level select
/// group — one no other group contains, e.g. "🌍 出口" holding the fallback "🇺🇸 US" — then any
/// select group, then the first group. mihomo lists groups alphabetically, so "first group" alone
/// can land on an inner fallback such as "Ⓜ Meta".
fn main_group(state: &LiveState) -> Option<&crate::present::ProxyGroup> {
    if state.mode == "global" {
        return state.groups.iter().find(|g| g.name == "GLOBAL");
    }
    let groups = || state.groups.iter().filter(|g| g.name != "GLOBAL");
    let nested = |name: &str| groups().any(|g| g.nodes.iter().any(|n| n.name == name));
    groups()
        .find(|g| g.kind == "select" && !nested(&g.name))
        .or_else(|| groups().find(|g| g.kind == "select"))
        .or_else(|| groups().next())
        .or(state.groups.first())
}

fn current_node(state: &LiveState) -> Current {
    if state.mode == "direct" {
        return Current {
            group: String::new(),
            label: "直连".to_string(),
            delay: "—".to_string(),
        };
    }
    let Some(group) = main_group(state) else {
        return Current {
            group: String::new(),
            label: "还没有节点".to_string(),
            delay: "—".to_string(),
        };
    };
    let delay = delay_of(state, &group.now);
    Current {
        group: group.name.clone(),
        label: group.now_label.clone(),
        delay,
    }
}

/// Opening the nodes page or picking a group probes that group once (at most every 20 s per group),
/// so members show measured delays instead of "—".
fn auto_test_selected_group() {
    if PREVIEW_MODE.with(Cell::get) {
        return;
    }
    let Some(live) = LIVE.with(|l| l.borrow().clone()) else {
        return;
    };
    let name = {
        let s = live.lock().unwrap();
        if !s.awake {
            return;
        }
        let idx = SELECTED_GROUP.with(Cell::get);
        match s.groups.get(idx).or_else(|| s.groups.first()) {
            Some(g) => g.name.clone(),
            None => return,
        }
    };
    let due = AUTO_TESTED.with(|m| {
        let mut m = m.borrow_mut();
        let ok = m
            .get(&name)
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(20));
        if ok {
            m.insert(name.clone(), Instant::now());
        }
        ok
    });
    if due {
        send(MenuCommand::TestGroup(name));
    }
}

fn delay_of(state: &LiveState, name: &str) -> String {
    if let Some(text) = state.delays.get(name) {
        return text.clone();
    }
    state
        .groups
        .iter()
        .flat_map(|group| group.nodes.iter())
        .find(|node| node.name == name)
        .and_then(|node| node.delay_ms)
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_else(|| "—".to_string())
}

fn delay_number(text: &str) -> i32 {
    // "—" (not measured) / "超时" / "…" sort after every measured node.
    text.strip_suffix("ms")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(999)
}

/// PurrNode.verdict level 2 ("常掉线"): loss >= 3 % or jitter >= 20 ms. Drawn dashed.
fn shaky_probe(state: &LiveState, name: &str) -> bool {
    let Some(history) = state.node_history.get(name) else {
        return false;
    };
    if history.is_empty() {
        return false;
    }
    let values = history
        .iter()
        .flatten()
        .map(|v| *v as f64)
        .collect::<Vec<_>>();
    let loss = (history.len() - values.len()) as f64 / history.len() as f64 * 100.0;
    if loss >= 3.0 {
        return true;
    }
    if values.len() < 2 {
        return false;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64).sqrt() >= 20.
}
fn publish_spectrum(ui: &MainWindow, state: &LiveState) {
    if state.groups.is_empty() {
        design::groups(ui, state, 0);
        publish(&GROUP_TABS_CACHE, vec![], |m| ui.set_group_tabs(m));
        publish(&SPECTRUM_CACHE, vec![], |m| ui.set_spectrum_nodes(m));
        return;
    }
    let sel_idx = SELECTED_GROUP.with(|slot| slot.get());
    let group_idx = if sel_idx < state.groups.len() {
        sel_idx
    } else {
        0
    };
    let group = &state.groups[group_idx];
    design::groups(ui, state, group_idx);

    let group_tabs = state
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| GroupTab {
            id: i as i32,
            name: g.label.as_str().into(),
            selected: i == group_idx,
            node: g.now_label.as_str().into(),
            delay: if shaky_probe(state, &g.now) {
                format!("{} · 抖", delay_of(state, &g.now))
            } else {
                delay_of(state, &g.now)
            }
            .into(),
            bad: state.trouble == "timeout" && g.now_label == current_node(state).label,
        })
        .collect::<Vec<_>>();
    publish(&GROUP_TABS_CACHE, group_tabs, |model| {
        ui.set_group_tabs(model)
    });

    ui.set_testing_group(state.testing.contains(&group.name));

    if ui.get_page() == 1 {
        ui.set_current_group_name(group.name.as_str().into());
        // 顶栏徽章始终是主组当前节点（与 Mac currentPrimaryNode 同源，名字和延迟一起）；
        // 窝点页「当前 组 → 节点」单独写 g-now-a，不再覆盖徽章的名字。
        ui.global::<S>()
            .set_g_now_a(group.now_label.as_str().into());
    }
    let heights = [252.0, 204.0, 156.0, 108.0, 62.0];
    let mut sorted_nodes = group
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let delay_text = delay_of(state, &node.name);
            let delay_num = delay_number(&delay_text);
            (i, node, delay_text, delay_num)
        })
        .collect::<Vec<_>>();
    sorted_nodes.sort_by_key(|item| item.3);

    let total_nodes = sorted_nodes.len();
    let slowest = sorted_nodes
        .iter()
        .map(|n| n.3)
        .filter(|d| *d < 999)
        .max()
        .unwrap_or(0);
    ui.global::<S>().set_sp_top(spectrum_top(slowest));
    let spectrum = sorted_nodes
        .iter()
        .enumerate()
        .map(|(rank, (idx, node, delay_text, delay_num))| {
            let h = heights[rank % 5];
            let raw_x = present::spectrum_position(*delay_num as f32);
            let rank_x = if total_nodes > 1 {
                0.04 + (rank as f32 / (total_nodes - 1) as f32) * 0.88
            } else {
                raw_x
            };
            let stem_x = (rank_x * 0.55 + raw_x * 0.45).clamp(0.03, 0.95);
            let samples = state.node_history.get(&node.name);
            let values = samples
                .into_iter()
                .flatten()
                .flatten()
                .map(|v| *v as f64)
                .collect::<Vec<_>>();
            let jitter = if values.len() >= 2 {
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                format!(
                    "{:.0}",
                    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64)
                        .sqrt()
                )
            } else {
                String::new()
            };
            let loss = samples
                .filter(|v| !v.is_empty())
                .map(|v| {
                    format!(
                        "{:.0}",
                        v.iter().filter(|s| s.is_none()).count() as f64 / v.len() as f64 * 100.
                    )
                })
                .unwrap_or_default();
            NodeItem {
                id: *idx as i32,
                name: node.label.as_str().into(),
                real_name: node.name.as_str().into(),
                group: group.name.as_str().into(),
                kind: node.kind.as_str().into(),
                delay: *delay_num,
                delay_text: delay_text.as_str().into(),
                selected: node.name == group.now,
                shaky: shaky_probe(state, &node.name),
                jitter: jitter.into(),
                loss: loss.into(),
                stem_height: h,
                stem_x,
                col: (*idx % 4) as i32,
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    publish(&SPECTRUM_CACHE, spectrum, |model| {
        ui.set_spectrum_nodes(model)
    });

    let list = group
        .nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            let delay_text = delay_of(state, &node.name);
            let delay_num = delay_number(&delay_text);
            NodeItem {
                id: idx as i32,
                name: node.label.as_str().into(),
                real_name: node.name.as_str().into(),
                group: group.name.as_str().into(),
                kind: node.kind.as_str().into(),
                delay: delay_num,
                delay_text: delay_text.as_str().into(),
                selected: node.name == group.now,
                stem_height: 50.0,
                stem_x: 0.0,
                col: (idx % 4) as i32,
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    publish(&LIST_CACHE, list, |model| ui.set_list_nodes(model));
}

fn publish_conns(ui: &MainWindow, state: &LiveState) {
    let query = CONN_QUERY.with(|query| query.borrow().clone());
    let rows = state
        .connections
        .iter()
        .filter(|conn| {
            query.is_empty()
                || contains(&conn.host, &query)
                || contains(&conn.app, &query)
                || contains(&conn.rule, &query)
                || contains(&conn.policy, &query)
        })
        .map(conn_row)
        .collect::<Vec<_>>();
    publish(&CONNS, rows, |model| ui.set_conns(model));
}

fn publish_rules(ui: &MainWindow, state: &LiveState) {
    let query = RULE_QUERY.with(|query| query.borrow().clone());
    if RULE_INPUT.with(|s| {
        s.borrow().as_ref().is_some_and(|last| {
            Arc::ptr_eq(&last.rules, &state.rules)
                && last.query == query
                && last.hits == state.telemetry.hits
        })
    }) {
        return;
    }
    let matched = state
        .rules
        .iter()
        .filter(|rule| {
            query.is_empty()
                || contains(&rule.kind, &query)
                || contains(&rule.payload, &query)
                || contains(&rule.policy, &query)
        })
        .collect::<Vec<_>>();
    let total = state.rules.len();
    let shown = matched.len();
    ui.set_rule_stat(
        if query.is_empty() {
            format!("{total} 条 · 显示 {shown}")
        } else {
            format!("{total} 条 · 匹配 {shown}")
        }
        .into(),
    );
    let rows = matched
        .into_iter()
        .map(|rule| RuleRow {
            index: rule.index.as_str().into(),
            kind: rule.kind.as_str().into(),
            payload: rule.payload.as_str().into(),
            policy: rule.policy.as_str().into(),
            tone: rule.tone,
            hits: state
                .telemetry
                .hits
                .iter()
                .filter(|(key, _)| {
                    let (kind, payload) = key.split_once(':').unwrap_or(("", ""));
                    kind.replace('-', "")
                        .eq_ignore_ascii_case(&rule.kind.replace('-', ""))
                        && (payload == rule.payload || (rule.kind == "MATCH" && payload.is_empty()))
                })
                .map(|(_, n)| *n)
                .sum::<u64>()
                .to_string()
                .into(),
        })
        .collect::<Vec<_>>();
    publish(&RULES, rows, |model| ui.set_rules(model));
    RULE_INPUT.with(|s| {
        *s.borrow_mut() = Some(RuleInput {
            rules: Arc::clone(&state.rules),
            query,
            hits: state.telemetry.hits.clone(),
        })
    });
}

fn publish_logs(ui: &MainWindow, state: &LiveState) {
    let filter = LOG_FILTER.with(|filter| filter.borrow().clone());
    let rows = state
        .logs
        .iter()
        .rev()
        .filter(|line| match filter.as_str() {
            "warn" => line.level == "warn" || line.level == "error",
            "purr" => line.level == "purr",
            _ => true,
        })
        .take(200)
        .map(|line| LogRow {
            time: line.time.as_str().into(),
            level: line.level.to_uppercase().into(),
            message: line.message.as_str().into(),
        })
        .collect::<Vec<_>>();
    publish(&LOGS, rows, |model| ui.set_logs(model));
}

fn inspect_live(ui: &MainWindow, state: &LiveState, id: i32) {
    STABLE_NODE.with(|s| s.borrow_mut().take());
    let Some(group) = state.groups.get(SELECTED_GROUP.with(Cell::get)) else {
        return;
    };
    let node = if id < 0 {
        group.nodes.iter().find(|n| n.name == group.now)
    } else {
        group.nodes.get(id as usize)
    };
    let Some(node) = node else {
        return;
    };
    ui.global::<S>().set_nd_lbl(
        if node.name == group.now {
            "当前窝点"
        } else {
            "看看这个"
        }
        .into(),
    );
    let history = state
        .node_history
        .get(&node.name)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let values: Vec<_> = history.iter().flatten().copied().collect();
    ui.set_detail_name(node.label.as_str().into());
    ui.set_detail_kind(format!("{} · {}", node.kind, group.label).into());
    ui.set_detail_delay(delay_of(state, &node.name).trim_end_matches("ms").into());
    // 与 Mac 同源：探测成功率 = 本次运行最近 N 次测速里有回应的比例（N 为真实样本数）
    ui.global::<S>().set_nd_n(history.len() as i32);
    ui.set_detail_uptime(
        if history.is_empty() {
            "—".to_string()
        } else {
            format!("{:.1}", values.len() as f64 / history.len() as f64 * 100.0)
        }
        .into(),
    );
    ui.set_detail_alt("".into());
    ui.set_detail_failures("M0 0".into());
    if values.len() < 2 {
        ui.set_detail_jitter("—".into());
        ui.set_detail_loss("—".into());
        ui.set_detail_verdict("样本不足".into());
        ui.set_detail_note("多次测速后查看抖动与丢包".into());
        ui.set_detail_spark("M0 22 H230".into());
        ui.global::<S>().set_sp_dot(-1.0);
        return;
    }
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64;
    let jitter = (values
        .iter()
        .map(|v| (*v as f64 - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64)
        .sqrt();
    let loss = (history.len() - values.len()) as f64 / history.len() as f64 * 100.0;
    ui.set_detail_jitter(format!("±{jitter:.0}").into());
    ui.set_detail_loss(format!("{loss:.0}").into());
    ui.set_detail_verdict(
        if loss >= 3.0 || jitter >= 20.0 {
            "常掉线"
        } else if loss >= 1.0 || jitter >= 10.0 {
            "有点抖"
        } else {
            "很稳"
        }
        .into(),
    );
    ui.set_detail_note(format!("基于最近 {} 次实测", history.len()).into());
    if loss >= 1. || jitter >= 10. {
        if let Some(alt) = group
            .nodes
            .iter()
            .filter(|n| n.name != node.name)
            .filter_map(|n| {
                let h = state.node_history.get(&n.name)?;
                let v = h.iter().flatten().copied().collect::<Vec<_>>();
                if v.len() < 2 || v.len() != h.len() {
                    return None;
                }
                let avg = v.iter().map(|v| *v as f64).sum::<f64>() / v.len() as f64;
                let dev = (v.iter().map(|v| (*v as f64 - avg).powi(2)).sum::<f64>()
                    / v.len() as f64)
                    .sqrt();
                (dev < 10.).then_some((n, avg))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            ui.set_detail_alt(format!("同组更稳：{} · {:.0}ms →", alt.0.label, alt.1).into());
            STABLE_NODE.with(|s| *s.borrow_mut() = Some((group.name.clone(), alt.0.name.clone())));
        }
    }
    let samples: Vec<Option<f32>> = history.iter().map(|v| (*v).map(|v| v as f32)).collect();
    let (path, failures, avg_y) = spark_paths(&samples);
    ui.global::<S>().set_sp_dot(avg_y);
    ui.set_detail_spark(path.into());
    ui.set_detail_failures(failures.into());
}

/// NodeSparklineView in a 230 × 44 box: y = h - (v - mn) / (mx - mn) * (h - 8) - 4 with
/// mx = max * 1.15 and mn = min * 0.85; × marks for failed probes; returns the average's y.
pub(crate) fn spark_paths(history: &[Option<f32>]) -> (String, String, f32) {
    let values: Vec<f32> = history.iter().flatten().copied().collect();
    if values.is_empty() || history.len() < 2 {
        return ("M0 0".into(), "M0 0".into(), -1.0);
    }
    let mx = values.iter().copied().fold(f32::MIN, f32::max) * 1.15;
    let mn = values.iter().copied().fold(f32::MAX, f32::min) * 0.85;
    let avg = values.iter().sum::<f32>() / values.len() as f32;
    let (w, h) = (230.0f32, 44.0f32);
    let y_at = |v: f32| h - (v - mn) / (mx - mn).max(1.0) * (h - 8.0) - 4.0;
    let x_at = |i: usize| i as f32 / (history.len() - 1) as f32 * w;
    let mut path = String::new();
    let mut failures = String::new();
    let mut drawing = false;
    for (i, v) in history.iter().enumerate() {
        let x = x_at(i);
        if let Some(v) = v {
            path.push_str(&format!(
                "{} {x:.2} {:.2} ",
                if drawing { "L" } else { "M" },
                y_at(*v)
            ));
            drawing = true;
        } else {
            drawing = false;
            failures.push_str(&format!(
                "M{:.2} {:.2} L{:.2} {:.2} M{:.2} {:.2} L{:.2} {:.2} ",
                x - 3.0,
                h - 7.0,
                x + 3.0,
                h - 1.0,
                x + 3.0,
                h - 7.0,
                x - 3.0,
                h - 1.0
            ));
        }
    }
    (
        if path.is_empty() { "M0 0".into() } else { path },
        if failures.is_empty() {
            "M0 0".into()
        } else {
            failures
        },
        y_at(avg),
    )
}

fn conn_row(conn: &Conn) -> ConnRow {
    ConnRow {
        id: conn.id.as_str().into(),
        app: conn.app.as_str().into(),
        host: conn.host.as_str().into(),
        rule: conn.rule.as_str().into(),
        policy: conn.policy.as_str().into(),
        kind: conn.kind,
        down: conn.down.as_str().into(),
        up: conn.up.as_str().into(),
        age: conn.age.as_str().into(),
    }
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn publish<T>(
    cache: &'static std::thread::LocalKey<RefCell<Option<Rc<VecModel<T>>>>>,
    rows: Vec<T>,
    apply_model: impl FnOnce(ModelRc<T>),
) where
    T: Clone + PartialEq + 'static,
{
    cache.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(model) = slot.as_ref() {
            if model.iter().eq(rows.iter().cloned()) {
                return;
            }
            model.set_vec(rows);
            return;
        }
        let model = Rc::new(VecModel::from(rows));
        apply_model(ModelRc::from(model.clone()));
        *slot = Some(model);
    });
}

fn elapsed_text() -> String {
    // 醒着却还没记下开始时间（界面先翻成醒着、或预览/验证直接置醒）：从现在开始计，
    // 不再一直停在 00:00:00。
    let awake_at = *AWAKE_AT
        .lock()
        .expect("awake lock")
        .get_or_insert_with(Instant::now);
    present::format_hms(awake_at.elapsed().as_secs())
}

fn clear_logs_local() {
    LIVE.with(|slot| {
        if let Some(live) = slot.borrow().as_ref() {
            let mut guard = live.lock().expect("live lock");
            guard.logs.clear();
            guard.bump();
        }
    });
    refresh_open();
}

fn send(command: MenuCommand) {
    if PREVIEW_MODE.with(Cell::get) {
        preview::command(command);
        return;
    }
    COMMANDS.with(|slot| {
        if let Some(sender) = slot.borrow().as_ref() {
            let _ = sender.send(command);
        }
    });
}

pub fn push_log(live: &Mutex<LiveState>, kind: &str, message: &str) {
    let mut guard = live.lock().expect("live lock");
    guard.logs.push(LogLine {
        time: present::clock_now(),
        level: log_level(kind).to_string(),
        message: message.to_string(),
    });
    if guard.logs.len() > 600 {
        let keep = guard.logs.len() - 500;
        guard.logs.drain(0..keep);
    }
    guard.bump();
}

/// Counts one rule-rejected connection from the core log (this run and today's telemetry).
pub fn count_reject(live: &Mutex<LiveState>) {
    let mut guard = live.lock().expect("live lock");
    guard.blocked_connections = guard.blocked_connections.saturating_add(1);
    guard.telemetry.reject(crate::subscriptions::now());
    guard.bump();
}

pub fn set_traffic_live(live: &Mutex<LiveState>, on: bool) {
    let mut guard = live.lock().expect("live lock");
    if guard.traffic_live != on {
        guard.traffic_live = on;
        if !on {
            guard.up_bps = 0;
            guard.down_bps = 0;
        }
        guard.bump();
    }
}

pub fn set_speed(live: &Mutex<LiveState>, up: u64, down: u64) {
    let mut guard = live.lock().expect("live lock");
    guard.traffic_live = true;
    guard.up_bps = up;
    guard.down_bps = down;
    let now = Instant::now();
    let tick = guard
        .last_sample
        .map(|then| now.saturating_duration_since(then) >= Duration::from_millis(400))
        .unwrap_or(true);
    if tick {
        guard.last_sample = Some(now);
        guard.history.push((up, down));
        if guard.history.len() > 120 {
            guard.history.remove(0);
        }
    }
    guard.bump();
}

#[allow(dead_code)]
pub fn set_flow(live: &Mutex<LiveState>, flow: f32) {
    let mut guard = live.lock().expect("live lock");
    guard.flow = flow.clamp(0.0, 1.0);
    guard.bump();
}

fn log_level(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "warning" | "warn" => "warn",
        "error" | "err" | "fatal" => "error",
        "purr" => "purr",
        _ => "info",
    }
}

pub fn set_directory(live: &Mutex<LiveState>, mode: &str, groups: Vec<ProxyGroup>) {
    let mut guard = live.lock().expect("live lock");
    if guard.mode == mode && guard.groups == groups {
        return;
    }
    let names = groups
        .iter()
        .flat_map(|group| group.nodes.iter().map(|node| node.name.clone()))
        .collect::<HashSet<_>>();
    guard.mode = mode.to_string();
    guard.groups = groups;
    guard.delays.retain(|name, _| names.contains(name));
    guard.bump();
}

pub fn set_delay(live: &Mutex<LiveState>, name: &str, delay: String) {
    let mut guard = live.lock().expect("live lock");
    if delay != "…" {
        let sample = delay.strip_suffix("ms").and_then(|s| s.parse::<u32>().ok());
        guard
            .telemetry
            .probe(name, sample.is_some(), crate::subscriptions::now());
        let history = guard.node_history.entry(name.into()).or_default();
        history.push(sample);
        if history.len() > 18 {
            history.remove(0);
        }
    }
    guard.delays.insert(name.to_string(), delay);
    guard.bump();
}

pub fn set_testing(live: &Mutex<LiveState>, group: &str, testing: bool) {
    let mut guard = live.lock().expect("live lock");
    let changed = if testing {
        guard.testing.insert(group.to_string())
    } else {
        guard.testing.remove(group)
    };
    if changed {
        guard.bump();
    }
}

pub fn begin_test(live: &Mutex<LiveState>, group: &str) -> Option<Vec<(String, String)>> {
    let mut guard = live.lock().expect("live lock");
    if guard.testing.contains(group) {
        return None;
    }
    let nodes = guard
        .groups
        .iter()
        .find(|item| item.name == group)
        .map(|item| {
            item.nodes
                .iter()
                .map(|node| (node.name.clone(), node.kind.clone()))
                .collect::<Vec<_>>()
        })?;
    guard.testing.insert(group.to_string());
    guard.bump();
    Some(nodes)
}

pub fn set_awake(live: &Mutex<LiveState>, awake: bool) {
    let mut guard = live.lock().expect("live lock");
    let mut awake_at = AWAKE_AT.lock().expect("awake lock");
    if awake {
        if awake_at.is_none() {
            *awake_at = Some(Instant::now());
        }
    } else {
        if let Some(started) = *awake_at {
            guard.last_nap = started.elapsed().as_secs();
        }
        *awake_at = None;
    }
    if guard.awake == awake {
        return;
    }
    guard.awake = awake;
    if !awake {
        guard.trouble.clear();
        guard.swap_at = None;
    }
    crate::sound::play(&guard.preferences.sound, awake);
    guard.bump();
}

pub fn set_memory(live: &Mutex<LiveState>, memory: String) {
    let mut guard = live.lock().expect("live lock");
    if guard.memory == memory {
        return;
    }
    guard.memory = memory;
    guard.bump();
}

pub fn set_place(live: &Mutex<LiveState>, place: String) {
    let mut guard = live.lock().expect("live lock");
    if guard.place == place {
        return;
    }
    guard.place = place;
    guard.bump();
}

pub fn set_connections(live: &Mutex<LiveState>, value: &serde_json::Value) {
    let (connections, totals) = present::connections_from(value, std::time::SystemTime::now());
    let mut guard = live.lock().expect("live lock");
    guard
        .telemetry
        .connections(value, crate::subscriptions::now());
    let mut route_down = [0u64; 2];
    if let Some(conns) = value["connections"].as_array() {
        for conn in conns {
            let policy = conn["chains"][0].as_str().unwrap_or("");
            if policy.starts_with("REJECT") {
                continue;
            }
            let kind = usize::from(policy == "DIRECT");
            route_down[kind] = route_down[kind].saturating_add(
                conn["download"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(conn["upload"].as_u64().unwrap_or(0)),
            );
        }
    }
    guard.route_down = route_down;
    if guard.connections == connections
        && guard.session_down == totals.down
        && guard.session_up == totals.up
    {
        return;
    }
    guard.connections = connections;
    guard.session_down = totals.down;
    guard.session_up = totals.up;
    guard.bump();
}

pub fn set_rules(live: &Mutex<LiveState>, value: &serde_json::Value) {
    let rules = present::rules_from(value);
    let mut guard = live.lock().expect("live lock");
    if guard.rules.as_ref() == rules.as_slice() {
        return;
    }
    guard.rules = rules.into();
    guard.bump();
}

fn apply_sites(ui: &MainWindow, state: &LiveState) {
    let g = ui.global::<S>();
    if g.get_sites_testing() != state.sites_testing {
        g.set_sites_testing(state.sites_testing);
    }
    if state.sites.is_empty() {
        return;
    }
    let rows: Vec<SiteRow> = crate::probe::SITES
        .iter()
        .map(|(id, name, domain)| {
            let p = state.sites.iter().find(|p| p.id == *id);
            SiteRow {
                name: (*name).into(),
                host: (*domain).into(),
                via: p.map(|p| p.via_text()).unwrap_or_default().into(),
                ms: p.map(|p| p.ms_text()).unwrap_or_else(|| "—".into()).into(),
            }
        })
        .collect();
    g.set_sites(ModelRc::new(VecModel::from(rows)));
}

/// 光谱右端刻度：max(250, 最慢已测节点 × 1.08 向上取整到 50)。Mac `spectrumTop` 同式。
fn spectrum_top(slowest_ms: i32) -> i32 {
    if slowest_ms <= 0 {
        return 250;
    }
    let want = (slowest_ms as f64 * 1.08 / 50.0).ceil() as i32 * 50;
    want.max(250)
}
