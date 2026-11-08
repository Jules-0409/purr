// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[cfg(windows)]
#[path = "tray_win.rs"]
mod tray_win;

pub(super) fn tray_nodes(state: &LiveState) -> Vec<TrayNode> {
    let Some(group) = main_group(state) else {
        return vec![];
    };
    // Nested groups (a fallback "🇺🇸 US" inside "🌍 出口") are real choices: list them with the
    // delay of the node they currently use. Only DIRECT/REJECT/relay-like entries are hidden.
    let mut nodes = group
        .nodes
        .iter()
        .filter(|n| present::probe_delay(&n.kind))
        .collect::<Vec<_>>();
    nodes.sort_by_key(|n| {
        delay_of(state, &n.name)
            .strip_suffix("ms")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    let selected = nodes.iter().find(|n| n.name == group.now).copied();
    nodes.truncate(5);
    if let Some(selected) = selected {
        if !nodes.iter().any(|n| n.name == selected.name) {
            if nodes.len() == 5 {
                nodes.pop();
            }
            nodes.push(selected);
        }
    }
    nodes
        .into_iter()
        .map(|n| TrayNode {
            group: group.name.as_str().into(),
            node: n.name.as_str().into(),
            name: n.label.as_str().into(),
            delay: delay_of(state, &n.name).into(),
            selected: n.name == group.now,
            verdict: state
                .telemetry
                .uptime(&n.name)
                .filter(|v| *v < 97.)
                .map(|_| "有点抖")
                .unwrap_or("")
                .into(),
        })
        .collect()
}
/// Opening the popover probes its group once (at most every 20 s, shared with the nodes page)
/// when a listed member has no measured delay yet, so "—" only stays when a probe really fails.
fn auto_test_tray(state: &LiveState) {
    if PREVIEW_MODE.with(Cell::get) || !state.awake || state.mode == "direct" {
        return;
    }
    let nodes = tray_nodes(state);
    let Some(group) = nodes.first().map(|n| n.group.to_string()) else {
        return;
    };
    if !nodes.iter().any(|n| n.delay.as_str() == "—") {
        return;
    }
    let due = AUTO_TESTED.with(|m| {
        let mut m = m.borrow_mut();
        let ok = m
            .get(&group)
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(20));
        if ok {
            m.insert(group.clone(), Instant::now());
        }
        ok
    });
    if due {
        send(MenuCommand::TestGroup(group));
    }
}

pub(super) fn apply_tray(ui: &TrayWindow, state: &LiveState) {
    ui.global::<Theme>().set_awake(state.awake);
    ui.global::<Theme>().set_mode(state.mode.as_str().into());
    ui.global::<Theme>().set_trouble(!state.trouble.is_empty());
    #[cfg(windows)]
    {
        ui.global::<Theme>().set_shortcut("Ctrl+".into());
        ui.global::<Theme>().set_sans("Segoe UI".into());
        ui.global::<Theme>().set_mono("Consolas".into());
        // SF Mono advances 0.619 em, Consolas 0.550 em: track the difference so widths match macOS.
        ui.global::<Theme>().set_mono_track(0.069);
    }
    let first = state
        .groups
        .iter()
        .all(|g| g.nodes.iter().all(|n| present::skip_delay(&n.kind)))
        && state.preferences.subscription.is_none();
    ui.set_first_run(first);
    ui.set_awake(state.awake);
    ui.set_mode(state.mode.as_str().into());
    ui.set_sys_proxy(state.preferences.system_proxy);
    ui.set_tun_mode(state.tun);
    ui.set_tun_auth(crate::tray::tun_authorized());
    let current = current_node(state);
    let ip = state
        .place
        .split_once(" · ")
        .map(|(ip, _)| ip)
        .filter(|ip| ip.chars().next().is_some_and(|c| c.is_ascii_hexdigit()))
        .unwrap_or("");
    let with_ip = |s: String| {
        if ip.is_empty() {
            s
        } else {
            format!("{s} · {ip}")
        }
    };
    ui.set_subtitle(
        if first {
            "先导入订阅".to_string()
        } else if !state.trouble.is_empty() {
            trouble_text(state).0
        } else if state.awake && state.mode == "direct" {
            with_ip("不经过窝点".to_string())
        } else if state.awake {
            with_ip(format!("{} · {}", current.label, current.delay))
        } else {
            format!("系统代理已关 · 端口 {} 仍在", state.preferences.mixed_port)
        }
        .into(),
    );
    ui.set_metrics(
        format!(
            "↓ {:.1}  ↑ {:.1} MB/s · 内存 {:.0} MB · 今日 {}",
            state.down_bps as f64 / 1048576.,
            state.up_bps as f64 / 1048576.,
            (state.memory_shell + state.memory_core) as f64 / 1_048_576.0,
            present::size_text(state.telemetry.down + state.telemetry.up)
        )
        .into(),
    );
    let (title, _, action, _) = trouble_text(state);
    ui.set_trouble(state.trouble.as_str().into());
    ui.set_trouble_title(title.into());
    ui.set_trouble_action(action.into());
    ui.set_countdown(countdown(state).into());
    ui.set_down(format!("{:.1}", state.down_bps as f64 / 1048576.).into());
    ui.set_up(format!("{:.1}", state.up_bps as f64 / 1048576.).into());
    ui.set_memory(
        format!(
            "{:.0}",
            (state.memory_shell + state.memory_core) as f64 / 1_048_576.0
        )
        .into(),
    );
    let total = present::size_text(state.telemetry.down.saturating_add(state.telemetry.up));
    let (number, unit) = total.split_once(' ').unwrap_or(("0", "B"));
    ui.set_total(number.into());
    ui.set_unit(unit.into());
    ui.set_nodes(ModelRc::new(VecModel::from(tray_nodes(state))));
}
pub fn toggle(x: i32, y: i32) {
    let exists = TRAY_WINDOW.with(|s| s.borrow().is_some());
    if exists {
        dismiss();
        return;
    }
    // The press on the tray icon that just closed the popover (outside-click) must not reopen it.
    #[cfg(windows)]
    if tray_win::just_auto_dismissed() {
        return;
    }
    #[cfg(windows)]
    {
        tray_win::snapshot_overflow();
        tray_win::set_anchor((x, y));
    }
    let Ok(ui) = TrayWindow::new() else { return };
    ui.global::<IconClock>()
        .set_reduced(crate::status::reduced_motion());
    let focused = Rc::new(Cell::new(false));
    let weak = ui.as_weak();
    ui.window().on_winit_window_event(move |_, event| {
        if let winit::event::WindowEvent::Focused(active) = event {
            if *active {
                focused.set(true);
            } else if focused.get() {
                // Opening from the status item emits an initial unfocused event.
                // Allow the status-item press to toggle the old window before
                // checking focus. Opening on mouse-down avoids reopening it on
                // release after that press has already dismissed it.
                let weak = weak.clone();
                slint::Timer::single_shot(Duration::from_millis(100), move || {
                    if let Some(ui) = weak.upgrade() {
                        if ui
                            .window()
                            .with_winit_window(|w| !w.has_focus())
                            .unwrap_or(false)
                        {
                            dismiss();
                        }
                    }
                });
            }
        }
        slint::winit_030::EventResult::Propagate
    });
    ui.on_dismiss(dismiss);
    ui.on_select_node(|group, node| {
        send(MenuCommand::Select {
            group: group.to_string(),
            node: node.to_string(),
        });
        // Windows: like a native tray menu, close once an item has been executed.
        #[cfg(windows)]
        dismiss();
    });
    ui.on_action(|action, value| match action.as_str() {
        "toggle" => {
            send(MenuCommand::ToggleRun);
            #[cfg(windows)]
            dismiss();
        }
        "mode" => {
            send(MenuCommand::SetMode(value.to_string()));
            #[cfg(windows)]
            dismiss();
        }
        "proxy" => {
            send(MenuCommand::ToggleSystemProxy);
            #[cfg(windows)]
            dismiss();
        }
        "tun" => {
            send(MenuCommand::ToggleTun);
            #[cfg(windows)]
            dismiss();
        }
        "fix" => {
            dismiss();
            send(MenuCommand::Open);
            slint::Timer::single_shot(Duration::from_millis(50), || {
                with_ui(|ui| {
                    ui.set_page(0);
                    ui.invoke_action("fix".into(), "primary".into());
                })
            });
        }
        "open" => {
            dismiss();
            send(MenuCommand::Open);
        }
        "quit" => send(MenuCommand::Quit),
        "copy" => {
            let port = crate::config::mixed_port().to_string();
            super::preview::copy_proxy_port(&port);
            #[cfg(windows)]
            dismiss();
        }
        _ => {}
    });
    let (first_run, has_trouble, nodes_len) = if let Some(state) =
        LIVE.with(|s| s.borrow().as_ref().map(|s| s.lock().unwrap().clone()))
    {
        let first = state
            .groups
            .iter()
            .all(|g| g.nodes.iter().all(|n| present::skip_delay(&n.kind)))
            && state.preferences.subscription.is_none();
        let has_trouble = !state.trouble.is_empty();
        let nodes_len = tray_nodes(&state).len();
        apply_tray(&ui, &state);
        auto_test_tray(&state);
        (first, has_trouble, nodes_len)
    } else {
        (true, false, 0)
    };
    let fallback_h = 63.0
        + if has_trouble { 76.0 } else { 0.0 }
        + if first_run {
            56.0
        } else {
            218.0 + nodes_len as f32 * 26.0
        }
        + 106.0;
    #[cfg(windows)]
    ui.set_upward(true);
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        NSRunningApplication::currentApplication()
            .activateWithOptions(NSApplicationActivationOptions::empty());
    }
    position_tray_window(&ui, x, y, fallback_h, false);
    let _ = ui.show();
    position_tray_window(&ui, x, y, fallback_h, true);
    TRAY_WINDOW.with(|s| *s.borrow_mut() = Some(ui));
    let timer = slint::Timer::default();
    let started = Instant::now();
    let last_h = Rc::new(Cell::new(fallback_h));
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(32),
        move || {
            #[cfg(windows)]
            if tray_win::poll() {
                dismiss();
                return;
            }
            TRAY_WINDOW.with(|s| {
                if let Some(ui) = s.borrow().as_ref() {
                    // Windows: the winit window is created lazily on the event loop, so it is
                    // not there right after show(). As soon as it is, take the foreground (so
                    // deactivation is reported, as with native tray menus) and start the
                    // light-dismiss watch (outside clicks, Esc, lost foreground).
                    #[cfg(windows)]
                    if !tray_win::armed() {
                        if let Some(hwnd) = ui.window().with_winit_window(|w| {
                            w.focus_window();
                            u64::from(w.id()) as isize
                        }) {
                            tray_win::force_foreground(hwnd);
                            tray_win::arm(hwnd);
                        }
                    }
                    ui.global::<IconClock>()
                        .set_time(started.elapsed().as_secs_f32() * 1000.);
                    ui.global::<IconClock>().set_awake(ui.get_awake());
                    let desired = ui.get_desired_height();
                    if desired > 100.0 && (desired - last_h.get()).abs() > 0.5 {
                        last_h.set(desired);
                        position_tray_window(ui, x, y, desired, false);
                    }
                }
            });
        },
    );
    TRAY_TIMER.with(|s| *s.borrow_mut() = Some(timer));
}

fn position_tray_window(ui: &TrayWindow, x: i32, y: i32, fallback_logical_h: f32, focus: bool) {
    let desired = ui.get_desired_height();
    let logical_h = if desired > 100.0 {
        desired
    } else {
        fallback_logical_h
    };
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
                            let _: () = objc2::msg_send![
                                layer,
                                setMasksToBounds: objc2::runtime::Bool::YES
                            ];
                        }
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            use std::ffi::c_void;
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            #[link(name = "user32")]
            unsafe extern "system" {
                fn SetWindowPos(
                    hwnd: isize,
                    hwnd_insert_after: isize,
                    x: i32,
                    y: i32,
                    cx: i32,
                    cy: i32,
                    u_flags: u32,
                ) -> i32;
                fn GetWindowLongPtrW(hwnd: isize, n_index: i32) -> isize;
                fn SetWindowLongPtrW(hwnd: isize, n_index: i32, dw_new_long: isize) -> isize;
            }
            #[link(name = "dwmapi")]
            unsafe extern "system" {
                fn DwmSetWindowAttribute(
                    hwnd: isize,
                    dw_attribute: u32,
                    pv_attribute: *const c_void,
                    cb_attribute: u32,
                ) -> i32;
            }
            let scale = w.scale_factor().max(1.0);
            let width = (308.0 * scale).round() as i32;
            let height = (logical_h as f64 * scale).round() as i32;
            let margin = (8.0 * scale).round() as i32;
            unsafe {
                // Native tray-menu placement: just outside the taskbar on whichever edge it is,
                // anchored on the icon, inside the work area, clear of the overflow flyout.
                let (left, top, upward) = tray_win::place((x, y), width, height, margin);
                ui.set_upward(upward);
                ui.window().set_size(slint::PhysicalSize::new(
                    width.max(1) as u32,
                    height.max(1) as u32,
                ));
                ui.window()
                    .set_position(slint::PhysicalPosition::new(left, top));
                if let Ok(handle) = w.window_handle() {
                    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                        let hwnd = handle.hwnd.get();
                        const GWL_EXSTYLE: i32 = -20;
                        const WS_EX_TOOLWINDOW: isize = 0x00000080;
                        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                        if (ex_style & WS_EX_TOOLWINDOW) == 0 {
                            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW);
                        }
                        let corner_pref: u32 = 2; // DWMWCP_ROUND
                        let _ = DwmSetWindowAttribute(
                            hwnd,
                            33, // DWMWA_WINDOW_CORNER_PREFERENCE
                            (&corner_pref as *const u32).cast::<c_void>(),
                            std::mem::size_of::<u32>() as u32,
                        );
                        let border_color: u32 = 0x00171412; // COLORREF BGR for #121417 (T.ink)
                        let _ = DwmSetWindowAttribute(
                            hwnd,
                            34, // DWMWA_BORDER_COLOR
                            (&border_color as *const u32).cast::<c_void>(),
                            std::mem::size_of::<u32>() as u32,
                        );
                        let flags = if focus {
                            0x0040 /* SWP_SHOWWINDOW */
                        } else {
                            0x0010 | 0x0004 /* SWP_NOACTIVATE | SWP_NOZORDER */
                        };
                        let _ = SetWindowPos(
                            hwnd, -1, /* HWND_TOPMOST */
                            left, top, width, height, flags,
                        );
                        let _ = focus;
                    }
                }
            }
            if focus {
                w.focus_window();
            }
            return;
        }
        #[cfg(not(windows))]
        {
            if let Some(monitor) = w.current_monitor().or_else(|| w.primary_monitor()) {
                let scale = w.scale_factor().max(1.0);
                let origin = monitor.position();
                let size = monitor.size();
                let width = (308. * scale) as i32;
                let height = ((logical_h as f64 * scale).round() as i32)
                    .max(ui.window().size().height as i32)
                    .max(1);
                let margin = (8. * scale) as i32;
                let mid_y = origin.y + (size.height as i32) / 2;
                let upward = y >= mid_y;
                ui.set_upward(upward);
                let left = (x - width / 2).clamp(
                    origin.x + margin,
                    (origin.x + size.width as i32 - width - margin).max(origin.x + margin),
                );
                let min_y = origin.y + margin;
                let max_y = (origin.y + size.height as i32 - height - margin).max(min_y);
                let top = if upward {
                    (y - height - (6. * scale) as i32).clamp(min_y, max_y)
                } else {
                    (y + (6. * scale) as i32).clamp(min_y, max_y)
                };
                ui.window()
                    .set_position(slint::PhysicalPosition::new(left, top));
            }
            if focus {
                w.focus_window();
            }
        }
    });
}
fn dismiss() {
    // Drop after focus/click callbacks finish to avoid borrowing the component during destruction.
    let _ = slint::invoke_from_event_loop(|| {
        #[cfg(windows)]
        tray_win::disarm();
        TRAY_TIMER.with(|s| s.borrow_mut().take());
        TRAY_WINDOW.with(|s| {
            if let Some(ui) = s.borrow_mut().take() {
                let _ = ui.hide();
            }
        });
    });
}
