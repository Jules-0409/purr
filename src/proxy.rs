// SPDX-License-Identifier: GPL-3.0-only

#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::Mutex;

use sysproxy::Sysproxy;

#[cfg(target_os = "macos")]
use crate::config::default_interface;
use crate::core;

struct SavedProxy {
    service: Option<String>,
    proxy: Sysproxy,
    #[cfg(target_os = "macos")]
    http: Sysproxy,
    #[cfg(target_os = "macos")]
    https: Sysproxy,
}

static PREVIOUS: Mutex<Option<SavedProxy>> = Mutex::new(None);

pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        core::terminate_running();
        previous_hook(info);
    }));
}

/// Only a loopback proxy on Purr's own mixed port counts as ours, so another
/// client's system proxy (e.g. FlClash on 7890) is never cleared or dropped.
fn is_loopback_proxy(p: &Sysproxy) -> bool {
    p.enable
        && matches!(p.host.as_str(), "127.0.0.1" | "localhost" | "::1")
        && p.port == crate::config::mixed_port()
}

fn sanitize_saved(mut p: Sysproxy) -> Sysproxy {
    if is_loopback_proxy(&p) {
        p.enable = false;
    }
    p
}

pub fn enable() -> Result<(), String> {
    let service = target_service()?;
    let current = read_proxy(service.as_deref())?;
    {
        let mut saved = PREVIOUS.lock().expect("proxy lock");
        if saved.is_none() {
            *saved = Some(SavedProxy {
                service: service.clone(),
                proxy: sanitize_saved(current.clone()),
                #[cfg(target_os = "macos")]
                http: sanitize_saved(
                    Sysproxy::get_http(service.as_deref().ok_or("network service missing")?)
                        .map_err(|e| e.to_string())?,
                ),
                #[cfg(target_os = "macos")]
                https: sanitize_saved(
                    Sysproxy::get_https(service.as_deref().ok_or("network service missing")?)
                        .map_err(|e| e.to_string())?,
                ),
            });
        }
    }
    let next = Sysproxy {
        enable: true,
        host: "127.0.0.1".to_string(),
        port: crate::config::mixed_port(),
        bypass: current.bypass,
    };
    if let Err(err) = write_proxy(service.as_deref(), &next) {
        restore();
        return Err(err);
    }
    println!("purr: system proxy enabled on configured loopback port");
    Ok(())
}

pub fn restore() {
    let previous = PREVIOUS.lock().expect("proxy lock").take();
    let Some(previous) = previous else {
        // Even if PREVIOUS was not recorded in this process, clear any stale loopback proxy left over from a killed process.
        if let Ok(service) = target_service() {
            if let Ok(mut current) = read_proxy(service.as_deref()) {
                if is_loopback_proxy(&current) {
                    current.enable = false;
                    let _ = write_proxy(service.as_deref(), &current);
                    println!("purr: stale loopback system proxy cleared");
                }
            }
        }
        return;
    };
    #[cfg(target_os = "macos")]
    let restored = if let Some(service) = previous.service.as_deref() {
        previous
            .proxy
            .set_socks(service)
            .and_then(|_| previous.http.set_http(service))
            .and_then(|_| previous.https.set_https(service))
            .and_then(|_| previous.proxy.set_bypass(service))
            .map_err(|e| e.to_string())
    } else {
        write_proxy(None, &previous.proxy)
    };
    #[cfg(not(target_os = "macos"))]
    let restored = write_proxy(previous.service.as_deref(), &previous.proxy);
    if let Err(err) = restored {
        eprintln!("purr: restore system proxy failed: {err}");
        *PREVIOUS.lock().expect("proxy lock") = Some(previous);
        return;
    }
    if previous.proxy.enable {
        println!(
            "purr: system proxy restored {}:{}",
            previous.proxy.host, previous.proxy.port
        );
    } else {
        println!("purr: system proxy cleared");
    }
}

fn target_service() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let device =
            default_interface().ok_or_else(|| "no physical default interface".to_string())?;
        let output = Command::new("networksetup")
            .arg("-listallhardwareports")
            .output()
            .map_err(|err| err.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "networksetup -listallhardwareports failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let text = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
        let service = service_from_hardware_ports(&text, &device)
            .ok_or_else(|| format!("no network service for {device}"))?;
        Ok(Some(service))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

#[cfg(any(target_os = "macos", test))]
fn service_from_hardware_ports(text: &str, device: &str) -> Option<String> {
    let mut port = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Hardware Port:") {
            port = Some(name.trim().to_string());
        } else if let Some(found) = line.strip_prefix("Device:") {
            if found.trim() == device {
                return port.filter(|name| !name.is_empty());
            }
        }
    }
    None
}

fn read_proxy(service: Option<&str>) -> Result<Sysproxy, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = service;
        Sysproxy::get_system_proxy().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    match service {
        Some(service) => {
            let mut proxy = Sysproxy::get_socks(service).map_err(|err| err.to_string())?;
            proxy.bypass = Sysproxy::get_bypass(service).map_err(|err| err.to_string())?;
            Ok(proxy)
        }
        None => Sysproxy::get_system_proxy().map_err(|err| err.to_string()),
    }
}

fn write_proxy(service: Option<&str>, proxy: &Sysproxy) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = service;
        proxy.set_system_proxy().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    match service {
        Some(service) => {
            proxy.set_socks(service).map_err(|err| err.to_string())?;
            proxy.set_https(service).map_err(|err| err.to_string())?;
            proxy.set_http(service).map_err(|err| err.to_string())?;
            proxy.set_bypass(service).map_err(|err| err.to_string())?;
            confirm(service, proxy)
        }
        None => proxy.set_system_proxy().map_err(|err| err.to_string()),
    }
}

#[cfg(target_os = "macos")]
fn confirm(service: &str, expected: &Sysproxy) -> Result<(), String> {
    for (kind, actual) in [
        ("http", Sysproxy::get_http(service)),
        ("https", Sysproxy::get_https(service)),
        ("socks", Sysproxy::get_socks(service)),
    ] {
        let actual = actual.map_err(|err| err.to_string())?;
        if actual.enable != expected.enable
            || actual.host != expected.host
            || actual.port != expected.port
        {
            return Err(format!(
                "{kind} proxy on {service} is {}:{} (enabled={}), expected {}:{} (enabled={})",
                actual.host,
                actual.port,
                actual.enable,
                expected.host,
                expected.port,
                expected.enable
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::service_from_hardware_ports;

    #[test]
    fn hardware_port_matches_the_device() {
        let text = "\
Hardware Port: Thunderbolt Bridge
Device: bridge0
Ethernet Address: aa:bb:cc:dd:ee:ff

Hardware Port: Wi-Fi
Device: en0
Ethernet Address: 11:22:33:44:55:66
";
        assert_eq!(
            service_from_hardware_ports(text, "en0").as_deref(),
            Some("Wi-Fi")
        );
        assert_eq!(service_from_hardware_ports(text, "en9"), None);
    }
}
