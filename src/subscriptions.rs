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
pub fn date(epoch: u64) -> String {
    let z = (epoch / 86400) as i64 + 719468;
    let era = z / 146097;
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
        assert_eq!(date(1893456000), "2030-01-01");
        assert!(!safe_web_url("file:///tmp/profile"));
        assert!(!safe_web_url("https://u:p@example.invalid"));
    }
}
