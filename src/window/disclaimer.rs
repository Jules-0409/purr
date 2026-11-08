//! First-launch notice. Runs its own event loop before anything else starts.
use super::*;

fn build() -> Result<DisclaimerWindow, String> {
    let ui = DisclaimerWindow::new().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        ui.global::<Theme>().set_mono("Consolas".into());
        ui.global::<Theme>().set_mono_track(0.069);
        ui.global::<Theme>().set_sans("Segoe UI".into());
    }
    let weak = ui.as_weak();
    ui.on_edited(move |text| {
        if let Some(ui) = weak.upgrade() {
            ui.set_phrase_ok(crate::settings::disclaimer_input_matches(&text));
        }
    });
    Ok(ui)
}

/// Shows the notice and returns true once the typed phrase was accepted and saved.
pub fn run(dir: &std::path::Path) -> Result<bool, String> {
    let ui = build()?;
    let accepted = Rc::new(Cell::new(false));
    let weak = ui.as_weak();
    let done = Rc::clone(&accepted);
    let dir = dir.to_path_buf();
    ui.on_accept(move || {
        let Some(ui) = weak.upgrade() else { return };
        if !crate::settings::disclaimer_input_matches(&ui.get_input()) {
            return;
        }
        match crate::settings::accept_disclaimer(&dir) {
            Ok(()) => {
                done.set(true);
                let _ = slint::quit_event_loop();
            }
            Err(e) => {
                eprintln!("purr: saving disclaimer acceptance failed: {e}");
                ui.set_save_failed(true);
            }
        }
    });
    ui.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    ui.show().map_err(|e| e.to_string())?;
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    let _ = ui.hide();
    Ok(accepted.get())
}

/// `--preview --disclaimer-capture <dir>`: renders the empty and the correctly typed state.
pub fn capture(out: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let ui = build()?;
    ui.show().map_err(|e| e.to_string())?;
    let weak = ui.as_weak();
    let out = out.to_path_buf();
    let step = Rc::new(Cell::new(0));
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let n = step.get();
            step.set(n + 1);
            let shot = |name: &str| {
                let path = out.join(format!("win-disclaimer-{name}.ppm"));
                if let Err(e) = snapshot(&ui, &path) {
                    eprintln!("disclaimer capture {name}: {e}");
                }
            };
            match n {
                1 => shot("empty"),
                2 => {
                    ui.set_input(crate::settings::DISCLAIMER_PHRASE.into());
                    ui.invoke_edited(crate::settings::DISCLAIMER_PHRASE.into());
                }
                4 => shot("typed"),
                5 => {
                    let _ = slint::quit_event_loop();
                }
                _ => {}
            }
        },
    );
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    drop(timer);
    let _ = ui.hide();
    Ok(())
}

fn snapshot(ui: &DisclaimerWindow, path: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    let frame = ui.window().take_snapshot().map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    write!(out, "P6\n{} {}\n255\n", frame.width(), frame.height()).map_err(|e| e.to_string())?;
    for px in frame.as_slice() {
        out.write_all(&[px.r, px.g, px.b])
            .map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())?;
    println!("disclaimer: captured {}", path.display());
    Ok(())
}
