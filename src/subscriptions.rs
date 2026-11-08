// SPDX-License-Identifier: GPL-3.0-only
use crate::settings::Subscription;
use reqwest::header::HeaderMap;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn url_name(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "订阅".into())
}
pub fn metadata(url: &str, headers: &HeaderMap, text: &str, id: u64) -> Subscription {
    let get = |key: &str| headers.get(key).and_then(|v| v.to_str().ok());
    let name = get("profile-title")
        .and_then(|s| {
            use base64::Engine;
            if let Some(encoded) = s.strip_prefix("base64:") {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
            } else {
                Some(decode(s))
            }
        })
        .or_else(|| {
            get("content-disposition").and_then(|s| {
                s.split(';').find_map(|part| {
                    let (k, v) = part.trim().split_once('=')?;
                    match k.to_ascii_lowercase().as_str() {
                        "filename*" => Some(decode(
                            v.trim_matches('"')
                                .strip_prefix("UTF-8''")
                                .unwrap_or(v.trim_matches('"')),
                        )),
                        "filename" => Some(decode(v.trim_matches('"'))),
                        _ => None,
                    }
                })
            })
        })
        .map(|s| {
            s.trim_end_matches(".yaml")
                .trim_end_matches(".yml")
                .chars()
                .filter(|c| !c.is_control())
                .take(80)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| url_name(url));
    let mut sub = Subscription {
        id,
        name,
        url: url.into(),
        updated: now(),
        ..Subscription::default()
    };
    if let Some(info) = get("subscription-userinfo") {
        for part in info.split(';') {
            if let Some((k, v)) = part.trim().split_once('=') {
                let v = v.trim().parse::<u64>().ok();
                match k.trim() {
                    "upload" => sub.upload = v,
                    "download" => sub.download = v,
                    "total" => sub.total = v,
                    "expire" => sub.expires = v,
                    _ => {}
                }
            }
        }
    }
    sub.nodes = serde_yaml::from_str::<serde_yaml::Value>(text)
        .ok()
        .and_then(|v| v.get("proxies").and_then(|n| n.as_sequence()).map(Vec::len))
        .unwrap_or(0);
    sub.homepage = get("profile-web-page-url")
        .filter(|s| safe_web_url(s))
        .map(str::to_owned);
    sub
}
pub fn safe_web_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}
fn decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut i = 0;
    let input = s.as_bytes();
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            if let Ok(hex) = std::str::from_utf8(&input[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    bytes.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        bytes.push(input[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
/// Calendar day (YYYY-MM-DD) of `epoch` on the user's own clock: the day the statistics roll on
/// and the day an expiry is shown as. UTC would roll "today" at 08:00 in China.
pub fn date(epoch: u64) -> String {
    civil_date(epoch as i64 + local_offset_secs(epoch))
}
/// Seconds east of UTC on the local clock at `epoch` (DST-aware); 0 when the OS cannot say.
pub fn local_offset_secs(epoch: u64) -> i64 {
    #[cfg(unix)]
    {
        let t = epoch as libc::time_t;
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return 0;
        }
        tm.tm_gmtoff as i64
    }
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            parts: [u16; 8],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn FileTimeToSystemTime(file: *const u64, out: *mut SystemTime) -> i32;
            fn SystemTimeToTzSpecificLocalTime(
                zone: *const std::ffi::c_void,
                utc: *const SystemTime,
                local: *mut SystemTime,
            ) -> i32;
            fn SystemTimeToFileTime(time: *const SystemTime, out: *mut u64) -> i32;
        }
        let utc_ft: u64 = epoch * 10_000_000 + 116_444_736_000_000_000;
        let mut utc = SystemTime::default();
        let mut local = SystemTime::default();
        let mut local_ft = 0u64;
        unsafe {
            if FileTimeToSystemTime(&utc_ft, &mut utc) == 0
                || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
                || SystemTimeToFileTime(&local, &mut local_ft) == 0
            {
                return 0;
            }
        }
        (local_ft as i64 - utc_ft as i64) / 10_000_000
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = epoch;
        0
    }
}
/// Proleptic Gregorian date of a (possibly offset) Unix second count.
pub fn civil_date(seconds: i64) -> String {
    let z = seconds.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_headers_supply_real_usage_and_expiry() {
        let mut h = HeaderMap::new();
        h.insert("profile-title", "base64:SGVsbG8=".parse().unwrap());
        h.insert(
            "subscription-userinfo",
            "upload=10; download=20; total=100; expire=1893456000"
                .parse()
                .unwrap(),
        );
        let s = metadata(
            "https://example.invalid/private?token=secret",
            &h,
            "proxies: [{name: test}]",
            3,
        );
        assert_eq!(s.name, "Hello");
        assert_eq!(s.used(), Some(30));
        assert_eq!(s.total, Some(100));
        assert_eq!(s.nodes, 1);
        assert_eq!(civil_date(1893456000), "2030-01-01");
        assert!(!safe_web_url("file:///tmp/profile"));
        assert!(!safe_web_url("https://u:p@example.invalid"));
    }
    #[test]
    fn today_rolls_on_the_local_clock_not_utc() {
        // 2026-11-07 16:30 UTC is 2026-11-08 00:30 in China (UTC+8).
        let epoch = 1_794_069_000;
        assert_eq!(civil_date(epoch), "2026-11-07");
        assert_eq!(civil_date(epoch + 8 * 3600), "2026-11-08");
        assert_eq!(civil_date(epoch - 17 * 3600), "2026-11-06");
        // `date` applies the machine's own offset: the result is UTC date shifted by it.
        let off = local_offset_secs(epoch as u64);
        assert!(off.abs() <= 14 * 3600);
        assert_eq!(date(epoch as u64), civil_date(epoch + off));
    }
}
