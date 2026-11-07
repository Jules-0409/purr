// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod api;
mod config;
mod core;
mod live;
mod present;
mod probe;
mod profile;
mod proxy;
mod settings;
mod sound;
mod status;
mod subscriptions;
mod telemetry;
mod tray;
mod verify;
mod window;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tray_icon::TrayIconBuilder;

use crate::api::Api;
use crate::core::Core;
use crate::tray::{
    build_menu, click_sender, quit_requested, serve, summary, tray_image, MenuActions, MenuCommand,
    MenuModel, SharedMenu,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|v| v == "--verify-backend") {
        if let Err(e) = args
            .get(i + 1)
            .ok_or("Missing fixture URL".into())
            .and_then(|url| verify::run(url))
        {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--preview") {
        if let Err(err) = window::preview() {
            eprintln!("purr preview: {err}");
            std::process::exit(1);
        }
        return;
    }
    proxy::install_panic_hook();
    if let Err(err) = run() {
        proxy::restore();
        core::terminate_running();
        eprintln!("purr: {err}");
        #[cfg(windows)]
        {
            #[link(name = "user32")]
            unsafe extern "system" {
                fn MessageBoxW(
                    window: *mut std::ffi::c_void,
                    text: *const u16,
                    title: *const u16,
                    kind: u32,
                ) -> i32;
            }
            let text: Vec<u16> = err.encode_utf16().chain(Some(0)).collect();
            let title: Vec<u16> = "PURR".encode_utf16().chain(Some(0)).collect();
            unsafe {
                MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
            }
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let dir = config::app_config_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let instance = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("instance.lock"))
        .map_err(|e| e.to_string())?;
    let enable_tun_arg = std::env::args().any(|a| a == "--enable-tun");
    let mut locked = false;
    for _ in 0..(if enable_tun_arg { 40 } else { 1 }) {
        if instance.try_lock().is_ok() {
            locked = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !locked {
        return Err("PURR 已经在运行，请使用托盘菜单打开主窗口".to_string());
    }
    proxy::restore();
    let settings = config::prepare()?;
    let api = Api::new(settings.secret.clone());
    let (commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    if enable_tun_arg {
        let _ = commands_tx.send(MenuCommand::ToggleTun);
    }
    let shared = Arc::new(Mutex::new(SharedMenu::default()));
    let preferences = settings::Preferences::load(&settings.config_dir)?;
    let live = Arc::new(Mutex::new(window::LiveState {
        preferences,
        telemetry: telemetry::Telemetry::load(&settings.config_dir),
        ..window::LiveState::default()
    }));

    let binary = settings.core_binary.clone();
    let dir = settings.config_dir.clone();
    let shared_worker = Arc::clone(&shared);
    let live_worker = Arc::clone(&live);
    let worker = thread::Builder::new()
        .name("purr-runtime".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            runtime.block_on(async move {
                let core = Core::start(binary, dir);
                serve(commands_rx, api, shared_worker, live_worker, core).await;
            });
        })
        .map_err(|err| err.to_string())?;

    slint::BackendSelector::new()
        .renderer_name(window::renderer_name())
        .select()
        .map_err(|err| err.to_string())?;

    let actions = Arc::new(Mutex::new(MenuActions::new()));
    let (menu, initial_actions) = build_menu(&MenuModel::default())?;
    *actions.lock().expect("menu lock") = initial_actions;
    let tray_builder = TrayIconBuilder::new().with_tooltip("Purr");
    #[cfg(target_os = "macos")]
    let tray_builder = tray_builder.with_icon_templated(tray_image());
    #[cfg(not(target_os = "macos"))]
    let tray_builder = tray_builder.with_icon(tray_image());
    let tray = tray_builder
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false);
    // Windows：右键也弹自绘弹窗（与 Mac 托盘弹窗一致），不再弹系统原生菜单。
    #[cfg(windows)]
    let tray = tray.with_menu_on_right_click(false);
    let tray = tray
        .build()
        .map_err(|err| err.to_string())?;
    tray_icon::TrayIconEvent::set_event_handler(Some(|event| {
        if let tray_icon::TrayIconEvent::Click {
            button,
            button_state: tray_icon::MouseButtonState::Down,
            rect,
            position,
            ..
        } = event
        {
            #[cfg(windows)]
            let wanted = matches!(button, tray_icon::MouseButton::Left | tray_icon::MouseButton::Right);
            #[cfg(not(windows))]
            let wanted = matches!(button, tray_icon::MouseButton::Left);
            if !wanted {
                return;
            }
            let has_rect = rect.size.width > 0
                && rect.size.height > 0
                && (rect.position.x != 0.0 || rect.position.y != 0.0);
            let x = if has_rect {
                (rect.position.x + rect.size.width as f64 / 2.0) as i32
            } else {
                position.x as i32
            };
            #[cfg(target_os = "macos")]
            let y = if has_rect {
                (rect.position.y + rect.size.height as f64) as i32
            } else {
                position.y as i32
            };
            #[cfg(not(target_os = "macos"))]
            let y = if has_rect {
                rect.position.y as i32
            } else {
                position.y as i32
            };
            let _ = slint::invoke_from_event_loop(move || window::toggle_tray(x, y));
        }
    }));

    muda::MenuEvent::set_event_handler(Some(click_sender(
        Arc::clone(&actions),
        commands_tx.clone(),
    )));

    window::init(
        commands_tx.clone(),
        Arc::clone(&live),
        present::short_path(&settings.config_dir),
    );
    let shared_timer = Arc::clone(&shared);
    let live_timer = Arc::clone(&live);
    let mut seen = 0u64;
    let mut dock_hidden = false;
    let mut opened_for_preview = false;
    let verify_ui = std::env::args().any(|a| a == "--runtime-ui-check")
        && settings
            .config_dir
            .join("purr-validation-fixture")
            .is_file();
    let mut verify_tick = 0u32;
    let fixture_dir = settings.config_dir.clone();
    let mut icon_state = (false, false);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || {
            if !dock_hidden {
                hide_dock_icon();
                dock_hidden = true;
            }
            if quit_requested() {
                let _ = slint::quit_event_loop();
                return;
            }
            if !opened_for_preview && !std::env::args().any(|a| a == "--minimized") {
                opened_for_preview = true;
                window::open();
            }
            window::sync(&live_timer);
            let next = {
                let s = live_timer.lock().unwrap();
                (s.awake, !s.trouble.is_empty())
            };
            if next != icon_state {
                icon_state = next;
                let _ = tray.set_icon(Some(tray::tray_image_state(next.0, next.1)));
            }
            if verify_ui {
                window::verify_runtime_tick(&fixture_dir);
                if !window::runtime_ui_driving()
                    && verify_tick < 30
                    && verify_tick.is_multiple_of(5)
                {
                    window::verify_tab((verify_tick / 5 % 6) as i32);
                }
                verify_tick += 1;
            }
            let model = {
                let guard = shared_timer.lock().expect("menu lock");
                if guard.generation == seen {
                    return;
                }
                seen = guard.generation;
                guard.model.clone()
            };
            match build_menu(&model) {
                Ok((menu, next_actions)) => {
                    *actions.lock().expect("menu lock") = next_actions;
                    tray.set_menu(Some(Box::new(menu)));
                    println!("purr: menu {}", summary(&model));
                }
                Err(err) => eprintln!("purr: menu build failed: {err}"),
            }
        },
    );

    let shell = std::process::id();
    println!("purr: tray shell={shell} mixed-port={}", config::MIXED_PORT);

    slint::run_event_loop_until_quit().map_err(|err| err.to_string())?;
    drop(timer);
    let _ = commands_tx.send(MenuCommand::Quit);
    let _ = worker.join();
    Ok(())
}

fn hide_dock_icon() {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let Some(marker) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(marker);
        let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}
