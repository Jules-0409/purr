// SPDX-License-Identifier: GPL-3.0-only
//! Preview-only parity scripts (`Purr --preview --script steps.txt --capture /abs/dir`).
//! Drives the fixture with real pointer/key events and saves cropped frames, so hover states and
//! animations can be compared frame by frame with the macOS snapshot runner. Never touches a core.
use super::*;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::LogicalPosition;
use std::io::Write;

struct Runner {
    steps: Vec<Vec<String>>,
    i: usize,
    wait_until: Instant,
    mark: Instant,
    burst: Option<(String, [f32; 4], Instant)>,
    out: std::path::PathBuf,
    frames: Vec<(String, u32, u32, Vec<u8>)>,
}

fn crop(ui: &MainWindow, r: [f32; 4]) -> Result<(u32, u32, Vec<u8>), String> {
    let frame = ui.window().take_snapshot().map_err(|e| e.to_string())?;
    let scale = frame.width() as f32
        / ui.window()
            .size()
            .to_logical(ui.window().scale_factor())
            .width;
    let (fw, fh) = (frame.width() as i32, frame.height() as i32);
    let x0 = ((r[0] * scale).round() as i32).clamp(0, fw);
    let y0 = ((r[1] * scale).round() as i32).clamp(0, fh);
    let x1 = (((r[0] + r[2]) * scale).round() as i32).clamp(0, fw);
    let y1 = (((r[1] + r[3]) * scale).round() as i32).clamp(0, fh);
    let px = frame.as_slice();
    let mut buf = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 3) as usize);
    for y in y0..y1 {
        for x in x0..x1 {
            let p = px[(y * fw + x) as usize];
            buf.extend_from_slice(&[p.r, p.g, p.b]);
        }
    }
    Ok(((x1 - x0) as u32, (y1 - y0) as u32, buf))
}

fn write_ppm(path: &std::path::Path, w: u32, h: u32, data: &[u8]) -> Result<(), String> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    write!(out, "P6\n{w} {h}\n255\n").map_err(|e| e.to_string())?;
    out.write_all(data).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())
}

fn rect(a: &[String]) -> [f32; 4] {
    let n = |i: usize| a.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    [n(2), n(3), n(4), n(5)]
}

fn key_text(k: &str) -> slint::SharedString {
    match k {
        "space" => " ".into(),
        "esc" => slint::platform::Key::Escape.into(),
        other => other.into(),
    }
}

pub fn run(
    script: &std::path::Path,
    out: &std::path::Path,
    live: Arc<Mutex<LiveState>>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(script).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let steps: Vec<Vec<String>> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().map(String::from).collect())
        .collect();
    let runner = Rc::new(RefCell::new(Runner {
        steps,
        i: 0,
        wait_until: Instant::now() + Duration::from_millis(800),
        mark: Instant::now(),
        burst: None,
        out: out.to_path_buf(),
        frames: Vec::new(),
    }));
    let timer = slint::Timer::default();
    let r2 = runner.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1),
        move || {
            let mut r = r2.borrow_mut();
            let now = Instant::now();
            if let Some((name, rc, until)) = r.burst.clone() {
                if now < until {
                    let t = now.saturating_duration_since(r.mark).as_millis() as u32;
                    let shot = WINDOW.with(|w| w.borrow().as_ref().map(|ui| crop(ui, rc)));
                    if let Some(Ok((w, h, d))) = shot {
                        r.frames.push((format!("{name}-{t:05}"), w, h, d));
                    }
                    return;
                }
                r.burst = None;
            }
            if now < r.wait_until {
                return;
            }
            while r.i < r.steps.len() {
                let a = r.steps[r.i].clone();
                r.i += 1;
                let arg = |i: usize| a.get(i).cloned().unwrap_or_default();
                let num = |i: usize| arg(i).parse::<f32>().unwrap_or(0.0);
                let pos = || LogicalPosition::new(num(1), num(2));
                let dispatch = |e: WindowEvent| with_ui(|ui| ui.window().dispatch_event(e));
                match a[0].as_str() {
                    "state" => {
                        super::preview::action("state", &arg(1));
                        sync(&live);
                    }
                    "page" => with_ui(|ui| {
                        let p = num(1) as i32;
                        ui.set_page(p);
                        ui.invoke_switch_page(p);
                    }),
                    "pane" => with_ui(|ui| ui.set_conn_pane(num(1) as i32)),
                    "reduced" => with_ui(|ui| ui.set_reduced_motion(arg(1) == "1")),
                    "toast" => toast(&a[1..].join(" ")),
                    "toggle" => super::preview::command(MenuCommand::ToggleRun),
                    "move" => dispatch(WindowEvent::PointerMoved { position: pos() }),
                    "leave" => dispatch(WindowEvent::PointerExited),
                    "down" => dispatch(WindowEvent::PointerPressed {
                        position: pos(),
                        button: PointerEventButton::Left,
                    }),
                    "up" => dispatch(WindowEvent::PointerReleased {
                        position: pos(),
                        button: PointerEventButton::Left,
                    }),
                    "click" => {
                        dispatch(WindowEvent::PointerMoved { position: pos() });
                        dispatch(WindowEvent::PointerPressed {
                            position: pos(),
                            button: PointerEventButton::Left,
                        });
                        dispatch(WindowEvent::PointerReleased {
                            position: pos(),
                            button: PointerEventButton::Left,
                        });
                    }
                    "keydown" => dispatch(WindowEvent::KeyPressed {
                        text: key_text(&arg(1)),
                    }),
                    "keyup" => dispatch(WindowEvent::KeyReleased {
                        text: key_text(&arg(1)),
                    }),
                    "mark" => {}
                    "wait" => {
                        r.wait_until = Instant::now() + Duration::from_millis(num(1) as u64);
                        return;
                    }
                    "cap" => {
                        let rc = rect(&a);
                        let name = arg(1);
                        let shot = WINDOW.with(|w| w.borrow().as_ref().map(|ui| crop(ui, rc)));
                        if let Some(Ok((w, h, d))) = shot {
                            r.frames.push((name, w, h, d));
                        }
                        continue;
                    }
                    "burst" => {
                        let rc = rect(&a);
                        let until = Instant::now() + Duration::from_millis(num(6) as u64);
                        r.burst = Some((arg(1), rc, until));
                        return;
                    }
                    other => eprintln!("script: unknown step {other}"),
                }
                // Every input/action step resets the burst clock: bursts are timed from the last action.
                r.mark = Instant::now();
            }
            let out = r.out.clone();
            let frames = std::mem::take(&mut r.frames);
            for (name, w, h, d) in &frames {
                if let Err(e) = write_ppm(&out.join(format!("{name}.ppm")), *w, *h, d) {
                    eprintln!("script: write {name}: {e}");
                }
            }
            println!("script: {} frames", frames.len());
            let _ = slint::quit_event_loop();
        },
    );
    slint::run_event_loop_until_quit().map_err(|e| e.to_string())?;
    drop(timer);
    stop_motion();
    WINDOW.with(|slot| slot.borrow_mut().take());
    Ok(())
}
