// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use serde_json::Value;

pub fn reduced_motion() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SystemParametersInfoW(action: u32, param: u32, value: *mut i32, flags: u32) -> i32;
        }
        let mut enabled = 1;
        unsafe { SystemParametersInfoW(0x1042, 0, &mut enabled, 0) != 0 && enabled == 0 }
    }
    #[cfg(target_os = "macos")]
    {
        use std::ffi::c_void;
        #[link(name = "CoreFoundation", kind = "framework")]
        unsafe extern "C" {
            fn CFStringCreateWithCString(
                allocator: *const c_void,
                text: *const std::ffi::c_char,
                encoding: u32,
            ) -> *const c_void;
            fn CFPreferencesGetAppBooleanValue(
                key: *const c_void,
                app: *const c_void,
                valid: *mut u8,
            ) -> u8;
            fn CFRelease(value: *const c_void);
        }
        unsafe {
            let key =
                CFStringCreateWithCString(std::ptr::null(), c"reduceMotion".as_ptr(), 0x08000100);
            let app = CFStringCreateWithCString(
                std::ptr::null(),
                c"com.apple.universalaccess".as_ptr(),
                0x08000100,
            );
            if key.is_null() || app.is_null() {
                return false;
            }
            let mut valid = 0;
            let value = CFPreferencesGetAppBooleanValue(key, app, &mut valid);
            CFRelease(key);
            CFRelease(app);
            valid != 0 && value != 0
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::env::var("GTK_ANIMATIONS").is_ok_and(|v| v == "0")
    }
}

pub fn resident_bytes(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    resident_bytes_impl(pid)
}
/// Memory the process really owns, as the OS task manager shows it:
/// macOS `phys_footprint` (Activity Monitor "Memory"), Windows private working set
/// (Task Manager "Memory"). Shared library pages are not counted, so adding the
/// shell and the core does not double-count them the way resident sizes do.
pub fn footprint_bytes(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    footprint_bytes_impl(pid).or_else(|| resident_bytes_impl(pid))
}

#[cfg(target_os = "macos")]
fn footprint_bytes_impl(pid: u32) -> Option<u64> {
    // rusage_info_v2: 16-byte uuid followed by u64 counters; phys_footprint is the 8th.
    #[repr(C)]
    struct RusageInfo {
        uuid: [u8; 16],
        counters: [u64; 40],
    }
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfo) -> i32;
    }
    const RUSAGE_INFO_V2: i32 = 2;
    let mut info = std::mem::MaybeUninit::<RusageInfo>::zeroed();
    let rc = unsafe { proc_pid_rusage(pid as i32, RUSAGE_INFO_V2, info.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let bytes = unsafe { info.assume_init() }.counters[7];
    (bytes > 0).then_some(bytes)
}

#[cfg(windows)]
fn footprint_bytes_impl(pid: u32) -> Option<u64> {
    use std::ffi::c_void;
    #[repr(C)]
    struct CountersEx2 {
        cb: u32,
        page_faults: u32,
        peak_working: usize,
        working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        commit: usize,
        peak_commit: usize,
        private: usize,
        private_working: usize,
        shared_commit: u64,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut CountersEx2,
            size: u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x1000 | 0x10, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: CountersEx2 = std::mem::zeroed();
        counters.cb = std::mem::size_of::<CountersEx2>() as u32;
        let ok = K32GetProcessMemoryInfo(handle, &mut counters, counters.cb);
        CloseHandle(handle);
        (ok != 0 && counters.private_working > 0).then_some(counters.private_working as u64)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn footprint_bytes_impl(_pid: u32) -> Option<u64> {
    None
}

/// Release allocator pages left by a window's initial resize and page teardown.
/// This does not purge live textures or reduce display resolution.
pub fn release_unused_memory() {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
    #[cfg(windows)]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut c_void;
            fn K32EmptyWorkingSet(process: *mut c_void) -> i32;
        }
        let _ = K32EmptyWorkingSet(GetCurrentProcess());
    }
}

#[cfg(target_os = "macos")]
fn resident_bytes_impl(pid: u32) -> Option<u64> {
    #[repr(C)]
    struct ProcTaskInfo {
        virtual_size: u64,
        resident_size: u64,
        total_user: u64,
        total_system: u64,
        threads_user: u64,
        threads_system: u64,
        policy: i32,
        faults: i32,
        pageins: i32,
        cow_faults: i32,
        messages_sent: i32,
        messages_received: i32,
        syscalls_mach: i32,
        syscalls_unix: i32,
        csw: i32,
        threadnum: i32,
        numrunning: i32,
        priority: i32,
    }

    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut ProcTaskInfo,
            buffersize: i32,
        ) -> i32;
    }

    const PROC_PIDTASKINFO: i32 = 4;
    let mut info = std::mem::MaybeUninit::<ProcTaskInfo>::zeroed();
    let wrote = unsafe {
        proc_pidinfo(
            pid as i32,
            PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr(),
            std::mem::size_of::<ProcTaskInfo>() as i32,
        )
    };
    if wrote <= 0 {
        return None;
    }
    Some(unsafe { info.assume_init() }.resident_size)
}

#[cfg(windows)]
fn resident_bytes_impl(pid: u32) -> Option<u64> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working: usize,
        working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        commit: usize,
        peak_commit: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut Counters, size: u32)
            -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x1000 | 0x10, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: Counters = std::mem::zeroed();
        counters.cb = std::mem::size_of::<Counters>() as u32;
        let ok = K32GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<Counters>() as u32,
        );
        CloseHandle(handle);
        (ok != 0).then_some(counters.working as u64)
    }
}
#[cfg(target_os = "linux")]
fn resident_bytes_impl(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|l| {
            l.strip_prefix("VmRSS:")
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<u64>().ok())
        })
        .map(|v| v * 1024)
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn resident_bytes_impl(_pid: u32) -> Option<u64> {
    None
}

pub fn format_memory(shell: Option<u64>, kernel: Option<u64>) -> String {
    format!("Purr {} · 内核 {}", megabytes(shell), megabytes(kernel))
}

fn megabytes(bytes: Option<u64>) -> String {
    match bytes {
        Some(bytes) => format!("{} MB", bytes / 1_000_000),
        None => "—".to_string(),
    }
}

pub fn format_place(ip: &str, city: &str, country: &str) -> String {
    let ip = ip.trim();
    let city = city.trim();
    let country = country.trim();
    if ip.is_empty() {
        return "暂时不知道".to_string();
    }
    match (city.is_empty(), country.is_empty()) {
        (true, true) => ip.to_string(),
        (false, true) => format!("{ip} · {city}"),
        (true, false) => format!("{ip} · {country}"),
        (false, false) => format!("{ip} · {city}, {country}"),
    }
}

/// The local broadband exit (no proxy): "ip · 城市 · CC · 本地宽带直连出口", like the macOS core.
pub async fn lookup_direct_place() -> Option<String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(4))
        .user_agent("purr")
        .build()
        .ok()?;
    for url in [
        "https://ipwho.is/",
        "http://ip-api.com/json?fields=status,query,city,countryCode",
        "https://ipinfo.io/json",
    ] {
        let Ok(resp) = client.get(url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(val) = resp.json::<Value>().await else {
            continue;
        };
        if let Some(place) = crate::present::place_from_json(&val) {
            return Some(format!("{place} · 本地宽带直连出口"));
        }
    }
    None
}

pub async fn lookup_place() -> String {
    let proxy = format!("http://127.0.0.1:{}", crate::config::mixed_port());
    let Ok(proxy) = reqwest::Proxy::all(&proxy) else {
        return "暂时不知道".to_string();
    };
    let Ok(client) = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(4))
        .user_agent("purr")
        .build()
    else {
        return "暂时不知道".to_string();
    };
    for url in [
        "https://ipwho.is/",
        "https://ifconfig.co/json",
        "https://ipinfo.io/json",
        "http://ip-api.com/json?fields=status,query,city,countryCode",
    ] {
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(value) = response.json::<Value>().await else {
            continue;
        };
        if let Some(place) = crate::present::place_from_json(&value) {
            return place;
        }
    }
    "暂时不知道".to_string()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateCheckResult {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub url: String,
    pub notes: String,
}

pub async fn check_remote_update() -> Result<UpdateCheckResult, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let target_url = "https://magpie.liujufu.dev/purr/version.json";
    let direct = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .user_agent("purr")
        .build()
        .map_err(|_| "无法发起更新检查请求".to_string())?;
    let resp = match direct.get(target_url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            let proxy_url = format!("http://127.0.0.1:{}", crate::config::mixed_port());
            let client = reqwest::Proxy::all(&proxy_url)
                .ok()
                .and_then(|p| {
                    reqwest::Client::builder()
                        .proxy(p)
                        .timeout(Duration::from_secs(5))
                        .user_agent("purr")
                        .build()
                        .ok()
                })
                .ok_or_else(|| "无法连接到更新服务器".to_string())?;
            client
                .get(target_url)
                .send()
                .await
                .map_err(|_| "无法连接到更新服务器".to_string())?
        }
    };
    if !resp.status().is_success() {
        return Err("更新服务器暂不可用".to_string());
    }
    let val: Value = resp
        .json()
        .await
        .map_err(|_| "更新信息解析失败".to_string())?;
    let latest = val
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or(&current)
        .trim_start_matches('v')
        .to_string();
    let url = val
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("https://magpie.liujufu.dev/purr/")
        .to_string();
    let notes = val
        .get("notes")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let has_update = is_newer_version(&latest, &current);
    Ok(UpdateCheckResult {
        current,
        latest,
        has_update,
        url,
        notes,
    })
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    parse(latest) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::{format_memory, format_place};

    #[test]
    fn current_process_reports_resident_memory() {
        let bytes = super::resident_bytes(std::process::id()).unwrap_or(0);
        assert!(bytes > 1024 * 1024, "resident {bytes}");
    }

    #[test]
    fn memory_uses_resident_megabytes() {
        assert_eq!(format_memory(Some(80_000_000), None), "Purr 80 MB · 内核 —");
    }

    #[test]
    fn place_joins_city_and_country() {
        assert_eq!(
            format_place("1.2.3.4", "Tokyo", "JP"),
            "1.2.3.4 · Tokyo, JP"
        );
        assert_eq!(format_place("", "Tokyo", "JP"), "暂时不知道");
    }
}
