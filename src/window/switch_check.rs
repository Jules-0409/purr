//! Exercise the real settings callback and the actual shared switch component.
use super::*;
use slint::platform::{PointerEventButton, WindowEvent};

pub fn run(path: &std::path::Path, live: &Arc<Mutex<LiveState>>) -> Result<(), String> {
    PREVIEW_MODE.with(|p| p.set(false));
    let mut optimistic = false;
    let mut after_refresh = false;
    let mut rapid_retained = false;
    let mut old_completion_retained = false;
    let mut failed_write_rolled_back = false;
    with_ui(|ui| {
        ui.set_page(5);
        let before = live.lock().unwrap().preferences.auto_swap;
        ui.global::<S>().invoke_switch_toggle(1);
        optimistic = ui.get_auto_swap() != before;
        // The 200 ms UI refresh may see the old preferences while validation,
        // config reload or disk persistence is still outstanding.
        apply(ui, &live.lock().unwrap());
        after_refresh = ui.get_auto_swap() != before;
        ui.global::<S>().invoke_switch_toggle(1);
        ui.global::<S>().invoke_switch_toggle(1);
        apply(ui, &live.lock().unwrap());
        rapid_retained = ui.get_auto_swap() != before;
    });
    finish_setting("auto-swap", 1);
    with_ui(|ui| {
        old_completion_retained = ui.get_auto_swap() != live.lock().unwrap().preferences.auto_swap
    });
    finish_setting("auto-swap", 3);
    with_ui(|ui| {
        failed_write_rolled_back = ui.get_auto_swap() == live.lock().unwrap().preferences.auto_swap
    });
    with_ui(|ui| {
        let _ = ui.hide();
    });
    stop_motion();
    let probe = SwitchCheckWindow::new().map_err(|e| e.to_string())?;
    let wave = present::ear_wave_v5(
        1200., 133.5, 500., 0., 0., true, 0., 0., "rule", true, false, 0., false,
    );
    probe.global::<S>().set_line_main(wave.main.into());
    probe.global::<S>().set_page(2);
    probe.global::<S>().set_grab_x(500.);
    probe.global::<S>().set_tip_x(592.);
    probe.show().map_err(|e| e.to_string())?;
    let mut tick = 0;
    let mut phase_start = probe.get_clock();
    let mut samples = Vec::new();
    let mut frames = Vec::new();
    let mut phase = "hover";
    let weak = probe.as_weak();
    let path = path.to_path_buf();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
        let probe = weak.upgrade().unwrap();
        if tick == 20 {
            probe.window().dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(140., 65.) });
            phase_start = probe.get_clock();
        }
        if [56, 98, 140, 144, 148].contains(&tick) {
            phase = match tick { 56 => "on", 98 => "off", 140 => "rapid-on", 144 => "rapid-off", _ => "rapid-on-again" };
            for pressed in [true, false] {
                let position = slint::LogicalPosition::new(140., 65.);
                let button = PointerEventButton::Left;
                probe.window().dispatch_event(if pressed {
                    WindowEvent::PointerPressed { position, button }
                } else {
                    WindowEvent::PointerReleased { position, button }
                });
            }
            phase_start = probe.get_clock();
        }
        if tick >= 20 { samples.push(serde_json::json!({
            "phase": phase, "clock": probe.get_clock(),
            "ms": probe.get_clock() - phase_start,
            "x": probe.get_knob_x(), "angle": probe.get_knob_angle(), "y": probe.get_knob_y(),
            "fill": probe.get_fill_progress(),
            "checked": probe.get_checked(), "hovered": probe.get_hovered()
        })); }
        if [20,28,36].contains(&tick) {
            use std::io::Write;
            let frame = probe.window().take_snapshot().unwrap();
            let image_path = path.with_file_name(format!("{}-{tick}.ppm",path.file_stem().unwrap().to_string_lossy()));
            let mut file = std::io::BufWriter::new(std::fs::File::create(&image_path).unwrap());
            write!(file,"P6\n{} {}\n255\n",frame.width(),frame.height()).unwrap();
            for p in frame.as_slice() { file.write_all(&[p.r,p.g,p.b]).unwrap(); }
            file.flush().unwrap();
            frames.push(serde_json::json!({"path":image_path,"angle":probe.get_knob_angle(),"y":probe.get_knob_y(),"scale":probe.window().scale_factor()}));
        }
        tick += 1;
        if tick == 190 {
            let report = serde_json::json!({"optimistic_toggle":optimistic,"retained_during_stale_refresh":after_refresh,
                "rapid_retained":rapid_retained,"old_completion_retained":old_completion_retained,"failed_write_rolled_back":failed_write_rolled_back,"samples":samples,"frames":frames});
            std::fs::write(&path,serde_json::to_vec_pretty(&report).unwrap()).unwrap();
            slint::quit_event_loop().unwrap();
        }
    });
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    drop(timer);
    stop_motion();
    WINDOW.with(|slot| slot.borrow_mut().take());
    Ok(())
}

pub fn waves(reference: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(reference).map_err(|e| e.to_string())?;
    let data: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut cases = Vec::new();
    for case in data["cases"].as_array().ok_or("missing reference cases")? {
        let i = &case["input"];
        let n = |key: &str| i[key].as_f64().unwrap() as f32;
        let b = |key: &str| i[key].as_bool().unwrap();
        let dashed = b("on") && !b("sys") && !b("tun");
        let mut paths = Vec::new();
        for dash in [false, dashed] {
            let wave = present::ear_wave_v5(
                n("width"),
                133.5,
                case["ear_x"].as_f64().unwrap() as f32,
                case["velocity"].as_f64().unwrap() as f32,
                n("time"),
                b("on"),
                n("hold"),
                n("load"),
                i["mode"].as_str().unwrap(),
                !i["err"].as_str().unwrap().is_empty(),
                case["reduced"].as_bool().unwrap(),
                n("twitch"),
                dash,
            );
            paths.push(serde_json::json!({"main":wave.main,"warm":wave.warm}));
        }
        cases.push(serde_json::json!({"name":i["name"],"solid":paths[0],"rendered":paths[1]}));
    }
    std::fs::write(
        output,
        serde_json::to_vec(&serde_json::json!({"cases":cases})).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    stop_motion();
    WINDOW.with(|slot| slot.borrow_mut().take());
    Ok(())
}
