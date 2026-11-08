// SPDX-License-Identifier: GPL-3.0-only
//! 常用网站真测：先经本地混合端口发一个 CONNECT，从 mihomo /connections 读出
//! 这条连接真实走的链路（规则匹配出的组/节点，或 DIRECT），再对那个组调用
//! `/proxies/{组}/delay?url=<网站>&timeout=5000`。直连的用 `DIRECT`。

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

use crate::api::Api;

/// (id, 显示名, 域名) —— 与 Mac `serviceProbes` 同一份列表、同一顺序。
pub const SITES: [(&str, &str, &str); 8] = [
    ("google", "Google", "www.google.com"),
    ("github", "GitHub", "api.github.com"),
    ("claude", "Claude", "claude.ai"),
    ("openai", "ChatGPT", "chatgpt.com"),
    ("youtube", "YouTube", "www.youtube.com"),
    ("telegram", "Telegram", "core.telegram.org"),
    ("twitter", "X / Twitter", "x.com"),
    ("apple", "Apple", "www.apple.com"),
];

/// 节流：自动测两次之间至少隔这么久（手动「重测」不受限）。概览页停留时也按这个间隔自动重测。
pub const THROTTLE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SiteProbe {
    pub id: String,
    pub name: String,
    pub domain: String,
    /// 实际走的组（链路最外层），直连为 "DIRECT"，查不到为空。
    pub group: String,
    /// 组里当前实际出口节点（链路最内层），与 group 相同则为空。
    pub node: String,
    /// None = 超时/失败。
    pub ms: Option<u32>,
}

impl SiteProbe {
    pub fn ms_text(&self) -> String {
        match self.ms {
            Some(ms) => format!("{ms}ms"),
            // 规则把它拦掉了（REJECT / REJECT-DROP）：不是超时
            None if self.blocked() => "已拦截".into(),
            // 连走哪条路都没查到（内核没就绪）：拿不到就是「—」，不算超时
            None if self.group.is_empty() => "—".into(),
            None => "超时".into(),
        }
    }
    /// The route is a REJECT policy: the site is blocked by a rule, not slow.
    pub fn blocked(&self) -> bool {
        self.group.starts_with("REJECT")
    }
    pub fn via_text(&self) -> String {
        if self.group.is_empty() {
            String::new()
        } else if self.blocked() {
            "抓老鼠".into()
        } else if self.group == "DIRECT" {
            "直连".into()
        } else {
            // 界面显示去掉 emoji 的名字（与窝点页一致）；测速仍用 mihomo 原名
            let g = crate::present::display_name(&self.group);
            if self.node.is_empty() {
                g
            } else {
                format!("{g} › {}", crate::present::display_name(&self.node))
            }
        }
    }
}

/// Opens `CONNECT domain:443` through the mixed port and keeps it open while the
/// caller looks the connection up. Returns the local port of the socket.
fn open_tunnel(port: u16, domain: &str) -> Option<(TcpStream, u16)> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(6)));
    let local = s.local_addr().ok()?.port();
    let req = format!("CONNECT {domain}:443 HTTP/1.1\r\nHost: {domain}:443\r\nProxy-Connection: keep-alive\r\n\r\n");
    s.write_all(req.as_bytes()).ok()?;
    // Read the status line; mihomo answers after it dialled (or failed to dial) upstream.
    let mut buf = [0u8; 256];
    let _ = s.read(&mut buf);
    Some((s, local))
}

fn chain_of(conns: &serde_json::Value, domain: &str, local: u16) -> Option<Vec<String>> {
    let list = conns.get("connections")?.as_array()?;
    let mut fallback = None;
    for c in list {
        let meta = c.get("metadata")?;
        let host = meta.get("host").and_then(|h| h.as_str()).unwrap_or("");
        if host != domain {
            continue;
        }
        let chains: Vec<String> = c
            .get("chains")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if chains.is_empty() {
            continue;
        }
        let sport = meta
            .get("sourcePort")
            .and_then(|p| {
                p.as_str()
                    .and_then(|s| s.parse::<u16>().ok())
                    .or(p.as_u64().map(|v| v as u16))
            })
            .unwrap_or(0);
        if sport == local {
            return Some(chains);
        }
        fallback.get_or_insert(chains);
    }
    fallback
}

/// Finds the real route for `domain` (outermost group, innermost node).
pub async fn route(api: &Api, port: u16, domain: &str) -> Option<(String, String)> {
    let d = domain.to_string();
    let tunnel = tokio::task::spawn_blocking(move || open_tunnel(port, &d))
        .await
        .ok()??;
    let (sock, local) = tunnel;
    let mut found = None;
    for _ in 0..12 {
        if let Ok(conns) = api.connections().await {
            if let Some(ch) = chain_of(&conns, domain, local) {
                found = Some(ch);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let _ = sock.shutdown(Shutdown::Both);
    let chains = found?;
    // mihomo chains: [节点, …, 最外层组]
    let group = chains.last().cloned().unwrap_or_default();
    let node = chains.first().cloned().unwrap_or_default();
    let node = if node == group { String::new() } else { node };
    Some((group, node))
}

/// Probes one site: real route, then the core's delay test through that route.
pub async fn probe_one(
    api: &Api,
    port: u16,
    id: &str,
    name: &str,
    domain: &str,
    mode: &str,
) -> SiteProbe {
    let (group, node) = match route(api, port, domain).await {
        Some(r) => r,
        None => match mode {
            // 查不到连接（例如内核刚重启）：按当前走法兜底，不猜规则。
            "direct" => ("DIRECT".into(), String::new()),
            "global" => ("GLOBAL".into(), String::new()),
            _ => (String::new(), String::new()),
        },
    };
    let ms = if group.starts_with("REJECT") || group.is_empty() {
        None
    } else {
        api.delay_url(&group, &format!("https://{domain}"), 5000)
            .await
            .ok()
    };
    SiteProbe {
        id: id.into(),
        name: name.into(),
        domain: domain.into(),
        group,
        node,
        ms,
    }
}

pub async fn probe_all(api: &Api, port: u16, mode: &str) -> Vec<SiteProbe> {
    let started = Instant::now();
    let futs = SITES
        .iter()
        .map(|(id, name, domain)| probe_one(api, port, id, name, domain, mode));
    let out = futures_util::future::join_all(futs).await;
    eprintln!(
        "purr: site probe {} in {}ms",
        out.iter()
            .map(|p| format!("{}={}@{}", p.id, p.ms_text(), p.group))
            .collect::<Vec<_>>()
            .join(" "),
        started.elapsed().as_millis()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::SiteProbe;

    fn probe(group: &str, ms: Option<u32>) -> SiteProbe {
        SiteProbe {
            group: group.into(),
            ms,
            ..Default::default()
        }
    }

    #[test]
    fn ms_text_never_invents_a_number() {
        assert_eq!(probe("节点选择", Some(123)).ms_text(), "123ms");
        // 路由查到了但测速没回：超时
        assert_eq!(probe("DIRECT", None).ms_text(), "超时");
        // 连路由都没查到（内核没就绪）：拿不到，显示「—」
        assert_eq!(probe("", None).ms_text(), "—");
        // 规则拦截：显示「已拦截」，不是「超时」
        assert_eq!(probe("REJECT", None).ms_text(), "已拦截");
        assert_eq!(probe("REJECT", None).via_text(), "抓老鼠");
    }
}
