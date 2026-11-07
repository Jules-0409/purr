// SPDX-License-Identifier: GPL-3.0-only
//! Bind the supplied native frontend to the application's existing command handlers.
use super::*;

pub(super) fn wire(ui: &MainWindow) {
    macro_rules! forward {
        ($on:ident, $invoke:ident $(, $arg:ident)*) => {
            let weak = ui.as_weak();
            ui.global::<S>().$on(move |$($arg),*| {
                if let Some(ui) = weak.upgrade() { ui.$invoke($($arg),*); }
            });
        };
    }
    forward!(on_tray_switch, invoke_toggle_run);
    forward!(on_set_mode, invoke_set_mode, mode);
    forward!(on_node_select, invoke_select_node, group, node);
    forward!(on_tray_node, invoke_select_node, group, node);
    forward!(on_test, invoke_test_group, group);
    forward!(on_pick_group, invoke_select_group, id);
    forward!(on_cut_all, invoke_close_connections);
    forward!(on_cut, invoke_close_connection, id);
    forward!(on_log_clear, invoke_clear_logs);
    forward!(on_rl_query, invoke_rule_query, query);
    forward!(on_cn_query, invoke_conn_query, query);
    forward!(on_log_filter, invoke_log_filter_changed, filter);
    forward!(on_toggle_sys, invoke_toggle_sys_proxy);
    forward!(on_toggle_tun, invoke_toggle_tun);
    forward!(on_action, invoke_action, action, value);
    forward!(on_window_action, invoke_window_action, action);
    forward!(on_drag_window, invoke_drag_window, dx, dy);

    ui.global::<S>().on_probe_sites(|force| send(MenuCommand::ProbeSites(force)));
    // 启动时停在概览：等内核就绪后自动测一次常用网站
    slint::Timer::single_shot(Duration::from_secs(5), || send(MenuCommand::ProbeSites(false)));
    ui.global::<S>().on_go(|page| {
        if page == 0 {
            send(MenuCommand::ProbeSites(false));
        }
        with_ui(|ui| {
            ui.set_page(page);
            ui.invoke_switch_page(page);
        })
    });
    ui.global::<S>().on_hover_node(|id| {
        with_ui(|ui| {
            ui.set_hover_node(id);
            ui.invoke_inspect_node(id);
        })
    });
    ui.global::<S>()
        .on_grab_begin(|| with_ui(|ui| ui.set_holding(true)));
    ui.global::<S>().on_grab_end(|| {
        with_ui(|ui| {
            ui.set_holding(false);
            ui.set_hold(0.);
        })
    });
    ui.global::<S>().on_do_import(|| {
        with_ui(|ui| {
            if !ui.get_busy() {
                ui.invoke_action("import".into(), ui.get_sub_editor_url());
            }
        })
    });
    ui.global::<S>().on_pick_node(|id| {
        with_ui(|ui| {
            if let Some(node) = ui.get_spectrum_nodes().iter().find(|n| n.id == id) {
                ui.invoke_select_node(node.group, node.real_name);
            }
        })
    });
    ui.global::<S>().on_run_test(|| {
        with_ui(|ui| {
            ui.invoke_test_group(ui.get_current_group_name());
        })
    });
    ui.global::<S>()
        .on_cn_pick(|pane| with_ui(|ui| ui.set_conn_pane(pane)));
    ui.global::<S>().on_lv_pick(|index| {
        with_ui(|ui| {
            ui.global::<S>().set_lv(index);
            ui.invoke_log_filter_changed(
                match index {
                    1 => "purr",
                    2 => "warn",
                    _ => "all",
                }
                .into(),
            );
        })
    });
    ui.global::<S>()
        .on_fix(|key| with_ui(|ui| ui.invoke_action("fix".into(), key)));
    ui.global::<S>()
        .on_toast_msg(|message| toast(message.as_str()));
    ui.global::<S>().on_switch_toggle(|index| {
        with_ui(|ui| {
            let key = match index {
                0 => {
                    ui.set_auto_wake(!ui.get_auto_wake());
                    "auto-wake"
                }
                1 => {
                    ui.set_auto_swap(!ui.get_auto_swap());
                    "auto-swap"
                }
                2 => {
                    ui.set_block_ads(!ui.get_block_ads());
                    "block-ads"
                }
                3 => {
                    ui.set_allow_lan(!ui.get_allow_lan());
                    "allow-lan"
                }
                4 => {
                    ui.set_ipv6(!ui.get_ipv6());
                    "ipv6"
                }
                _ => return,
            };
            ui.invoke_action("setting".into(), key.into());
        })
    });
    ui.global::<S>().on_snd_pick(|index| {
        with_ui(|ui| {
            let value = match index {
                1 => "ping",
                2 => "loop",
                _ => "off",
            };
            ui.invoke_action("sound".into(), value.into());
        })
    });
    ui.global::<S>().on_dns_pick(|index| {
        with_ui(|ui| {
            let value = match index {
                1 => "8.8.8.8",
                2 => "223.5.5.5",
                _ => "1.1.1.1",
            };
            ui.invoke_action("dns".into(), value.into());
        })
    });
    ui.global::<S>().on_close_pick(|index| {
        with_ui(|ui| {
            let to_tray = index == 0;
            ui.set_close_to_tray(to_tray);
            ui.invoke_action("setting".into(), "close-to-tray".into());
        })
    });
}

pub(super) fn groups(ui: &MainWindow, state: &LiveState, index: usize) {
    let labels = ui.global::<S>().get_groups();
    if labels.row_count() != state.groups.len()
        || labels
            .iter()
            .zip(&state.groups)
            .any(|(label, group)| label != group.label)
    {
        ui.global::<S>().set_groups(ModelRc::new(VecModel::from(
            state
                .groups
                .iter()
                .map(|g| g.label.as_str().into())
                .collect::<Vec<slint::SharedString>>(),
        )));
    }
    ui.global::<S>().set_gi(index as i32);
    let current = current_node(state);
    ui.global::<S>().set_status_detail(
        if state.awake {
            format!(
                " · {} · {} · {}",
                match state.mode.as_str() {
                    "global" => "全局",
                    _ => "分流",
                },
                current.label,
                current.delay
            )
        } else {
            " · OFF".into()
        }
        .into(),
    );
}
