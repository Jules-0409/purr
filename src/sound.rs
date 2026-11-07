// SPDX-License-Identifier: GPL-3.0-only
use std::sync::atomic::{AtomicU64, Ordering};
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn play(mode: &str, awake: bool) {
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    #[cfg(windows)]
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
    if mode == "off" || (mode == "loop" && !awake) {
        return;
    }
    let looping = mode == "loop";
    std::thread::spawn(move || {
        let Ok(dir) = crate::config::app_config_dir() else {
            return;
        };
        let path = dir.join(if looping {
            "purr-loop.wav"
        } else {
            "purr-ping.wav"
        });
        if !path.is_file() && crate::settings::atomic_write(&path, &wave(looping)).is_err() {
            return;
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if GENERATION.load(Ordering::Relaxed) == generation {
                unsafe {
                    PlaySoundW(
                        file.as_ptr(),
                        std::ptr::null_mut(),
                        0x20001 | if looping { 8 } else { 0 },
                    );
                }
            }
        }
        #[cfg(not(windows))]
        loop {
            if GENERATION.load(Ordering::Relaxed) != generation {
                break;
            }
            let mut command = std::process::Command::new(if cfg!(target_os = "macos") {
                "afplay"
            } else {
                "paplay"
            });
            let Ok(mut child) = command
                .arg(&path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            else {
                break;
            };
            loop {
                if GENERATION.load(Ordering::Relaxed) != generation {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                match child.try_wait() {
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                    _ => break,
                }
            }
            if !looping {
                break;
            }
        }
    });
}
#[cfg(windows)]
#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
}

fn wave(looping: bool) -> Vec<u8> {
    let rate = 24000u32;
    let samples = if looping { rate } else { rate / 4 };
    let bytes = samples * 2;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend(b"RIFF");
    out.extend((36 + bytes).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(bytes.to_le_bytes());
    for i in 0..samples {
        let t = i as f32 / rate as f32;
        let phase = t * std::f32::consts::TAU;
        let envelope = if looping {
            0.55 + 0.45 * (phase * 25.0).sin()
        } else {
            (-t * 18.0).exp() * (t * 120.0).min(1.0)
        };
        let value = ((phase * 120.0).sin() + 0.2 * (phase * 240.0).sin()) * envelope * 1800.0;
        out.extend((value as i16).to_le_bytes());
    }
    out
}
#[cfg(test)]
mod tests {
    #[test]
    fn audio_is_valid_mono_pcm() {
        for looping in [false, true] {
            let wav = super::wave(looping);
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            assert_eq!(
                u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
                wav.len() - 8
            );
            assert_eq!(
                u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
                wav.len() - 44
            );
        }
    }
}
