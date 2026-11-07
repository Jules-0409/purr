// SPDX-License-Identifier: GPL-3.0-only
//! Local native-UI driver, enabled only by --runtime-ui-check and a fixture marker.
use super::*;
thread_local! { static DRIVING: Cell<bool> = const { Cell::new(false) }; }
pub fn driving() -> bool {
    DRIVING.with(Cell::get)
}
pub fn tick(dir: &std::path::Path) {
    let request = dir.join("ui-request.json");
    if let Ok(bytes) = std::fs::read(&request) {
        if let Ok(r) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            DRIVING.with(|state| state.set(true));
            let _ = std::fs::remove_file(&request);
            let action = r["action"].as_str().unwrap_or("");
            let value = r["value"].as_str().unwrap_or("");
            if action == "open" {
                open();
            } else {
                with_ui(|ui| match action {
                    "import" => {
                        ui.set_page(2);
                        ui.set_sub_editor_url(value.into());
                        let weak = ui.as_weak();
                        slint::Timer::single_shot(Duration::from_millis(400), move || {
                            let Some(ui) = weak.upgrade() else { return };
                            assert_eq!(ui.get_page(), 2, "import page must remain selected");
                            let position = slint::LogicalPosition::new(
                                ui.global::<S>().get_import_x(),
                                ui.global::<S>().get_import_y(),
                            );
                            ui.window().dispatch_event(
                                slint::platform::WindowEvent::PointerPressed {
                                    position,
                                    button: slint::platform::PointerEventButton::Left,
                                },
                            );
                            ui.window().dispatch_event(
                                slint::platform::WindowEvent::PointerReleased {
                                    position,
                                    button: slint::platform::PointerEventButton::Left,
                                },
                            );
                        });
                    }
                    "page" => verify_tab(value.parse().unwrap_or(0)),
                    "rule-query" => ui.invoke_rule_query(value.into()),
                    "toggle-run" => ui.invoke_toggle_run(),
                    "port" => ui.invoke_action("port".into(), value.into()),
                    "file" => send(MenuCommand::ImportFile(std::path::PathBuf::from(value))),
                    "use" => ui.invoke_action("use-id".into(), value.into()),
                    "update" => ui.invoke_action("update-id".into(), value.into()),
                    "edit" => ui.invoke_action("edit-id".into(), value.into()),
                    "cancel-edit" => ui.invoke_action("cancel-edit".into(), "".into()),
                    "fix" => ui.invoke_action("fix".into(), value.into()),
                    "inspect" => ui.invoke_inspect_node(value.parse().unwrap_or(-1)),
                    "close" => ui.invoke_window_action("close".into()),
                    "tray" => ui.set_tray_open(value == "true"),
                    "native-tray" => toggle_tray(800, 32),
                    "native-tray-focus-main" => {
                        ui.window()
                            .with_winit_window(|window| window.focus_window());
                    }
                    "native-tray-mode" => TRAY_WINDOW.with(|slot| {
                        if let Some(tray) = slot.borrow().as_ref() {
                            let position = slint::LogicalPosition::new(154., 108.);
                            for pressed in [true, false] {
                                let button = slint::platform::PointerEventButton::Left;
                                tray.window().dispatch_event(if pressed {
                                    slint::platform::WindowEvent::PointerPressed {
                                        position,
                                        button,
                                    }
                                } else {
                                    slint::platform::WindowEvent::PointerReleased {
                                        position,
                                        button,
                                    }
                                });
                            }
                        }
                    }),
                    "native-tray-escape" => TRAY_WINDOW.with(|slot| {
                        if let Some(tray) = slot.borrow().as_ref() {
                            tray.window().dispatch_event(
                                slint::platform::WindowEvent::KeyPressed {
                                    text: slint::platform::Key::Escape.into(),
                                },
                            );
                        }
                    }),
                    "ui-sound" => ui.global::<S>().invoke_snd_pick(value.parse().unwrap_or(0)),
                    "ui-dns" => ui.global::<S>().invoke_dns_pick(value.parse().unwrap_or(0)),
                    "ui-switch" => ui
                        .global::<S>()
                        .invoke_switch_toggle(value.parse().unwrap_or(4)),
                    "ui-switch-burst" => {
                        for _ in 0..7 {
                            ui.global::<S>().invoke_switch_toggle(4);
                        }
                    }
                    _ => {}
                });
            }
            let _ = crate::settings::atomic_write(
                &dir.join("ui-accepted.json"),
                serde_json::to_string(&r["id"])
                    .unwrap_or_default()
                    .as_bytes(),
            );
        }
    }
    let mut snapshot = serde_json::json!({"open":false});
    with_ui(|ui| {
        let subs=ui.get_subscriptions().iter().map(|s|serde_json::json!({"id":s.id.as_str(),"name":s.title.as_str(),"used":s.used.as_str(),"total":s.total.as_str(),"expiry":s.expiry.as_str(),"active":s.active,"expired":s.expired})).collect::<Vec<_>>();
        snapshot = serde_json::json!({"open":true,"page":ui.get_page(),"first":ui.get_first_run(),"busy":ui.get_busy(),"trouble":ui.get_trouble().as_str(),"title":ui.get_trouble_title().as_str(),"subscriptions":subs,"port":ui.get_mixed_port().as_str(),"rules":ui.get_rules().row_count(),"toast":ui.get_toast_msg().as_str(),"uptime":ui.get_detail_uptime().as_str()});
    });
    snapshot["native_tray"] = TRAY_WINDOW.with(|slot| slot.borrow().is_some()).into();
    with_ui(|ui| snapshot["editing"] = ui.get_editing_subscription().as_str().into());
    with_ui(|ui| snapshot["ui_ipv6"] = ui.get_ipv6().into());
    snapshot["pending_settings"] = PENDING_SETTINGS
        .with(|pending| pending.borrow().len())
        .into();
    with_ui(|ui| {
        snapshot["import_xy"] = serde_json::json!([
            ui.global::<S>().get_import_x(),
            ui.global::<S>().get_import_y()
        ])
    });
    if let Some(state) = LIVE.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|live| live.lock().unwrap().clone())
    }) {
        snapshot["mode"] = state.mode.into();
        snapshot["sound"] = state.preferences.sound.into();
        snapshot["dns"] = state.preferences.dns.into();
        snapshot["ipv6"] = state.preferences.ipv6.into();
    }
    let _ = crate::settings::atomic_write(
        &dir.join("ui-snapshot.json"),
        serde_json::to_string(&snapshot).unwrap().as_bytes(),
    );
}
