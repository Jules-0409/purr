// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Telemetry {
    pub day: String,
    pub down: u64,
    pub up: u64,
    pub routes: [u64; 2],
    pub blocked: u64,
    pub hits: HashMap<String, u64>,
    pub health: HashMap<String, Vec<Bucket>>,
    #[serde(skip)]
    pub previous_totals: Option<(u64, u64)>,
    #[serde(skip)]
    pub previous_connections: HashMap<String, u64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bucket {
    pub start: u64,
    pub ok: u32,
    pub failed: u32,
}
impl Telemetry {
    pub fn roll(&mut self, now: u64) {
        let day = crate::subscriptions::date(now);
        if self.day != day {
            self.day = day;
            self.down = 0;
            self.up = 0;
            self.routes = [0; 2];
            self.blocked = 0;
            self.hits.clear();
        }
        self.health.retain(|_, buckets| {
            buckets.retain(|b| b.start + 300 > now.saturating_sub(86400));
            !buckets.is_empty()
        });
    }
    pub fn probe(&mut self, name: &str, ok: bool, now: u64) {
        self.roll(now);
        let buckets = self.health.entry(name.into()).or_default();
        let start = now / 300 * 300;
        if buckets.last().is_none_or(|b| b.start != start) {
            buckets.push(Bucket {
                start,
                ..Bucket::default()
            });
        }
        let b = buckets.last_mut().unwrap();
        if ok {
            b.ok = b.ok.saturating_add(1);
        } else {
            b.failed = b.failed.saturating_add(1);
        }
    }
    pub fn uptime(&self, name: &str) -> Option<f64> {
        let buckets = self.health.get(name)?;
        let ok = buckets.iter().map(|b| b.ok as u64).sum::<u64>();
        let all = buckets
            .iter()
            .map(|b| b.ok as u64 + b.failed as u64)
            .sum::<u64>();
        (all > 0).then_some(ok as f64 / all as f64 * 100.)
    }
    pub fn connections(&mut self, v: &serde_json::Value, now: u64) {
        self.roll(now);
        let down = v["downloadTotal"].as_u64().unwrap_or(0);
        let up = v["uploadTotal"].as_u64().unwrap_or(0);
        {
            let (d, u) = self.previous_totals.unwrap_or((0, 0));
            self.down = self
                .down
                .saturating_add(if down >= d { down - d } else { down });
            self.up = self.up.saturating_add(if up >= u { up - u } else { up });
        }
        self.previous_totals = Some((down, up));
        let mut seen = HashMap::new();
        if let Some(items) = v["connections"].as_array() {
            for c in items {
                let Some(id) = c["id"].as_str() else { continue };
                let bytes = c["download"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(c["upload"].as_u64().unwrap_or(0));
                let previous = self.previous_connections.get(id).copied();
                let policy = c["chains"][0].as_str().unwrap_or("");
                if previous.is_none() {
                    let key = format!(
                        "{}:{}",
                        c["rule"].as_str().unwrap_or(""),
                        c["rulePayload"].as_str().unwrap_or("")
                    );
                    *self.hits.entry(key).or_default() += 1;
                }
                if !policy.starts_with("REJECT") {
                    let k = usize::from(policy == "DIRECT");
                    self.routes[k] =
                        self.routes[k].saturating_add(bytes.saturating_sub(previous.unwrap_or(0)));
                }
                seen.insert(id.into(), bytes);
            }
        }
        self.previous_connections = seen;
    }
    /// One rule-rejected connection, seen in the core's info log (see `present::is_reject_log`).
    pub fn reject(&mut self, now: u64) {
        self.roll(now);
        self.blocked = self.blocked.saturating_add(1);
    }
    pub fn load(dir: &std::path::Path) -> Self {
        let mut data = std::fs::read(dir.join("statistics.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self::roll(&mut data, crate::subscriptions::now());
        data
    }
    pub fn save(&self, dir: &std::path::Path) -> Result<(), String> {
        crate::settings::atomic_write(
            &dir.join("statistics.json"),
            &serde_json::to_vec(self).map_err(|e| e.to_string())?,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_connections_do_not_duplicate_hits_and_day_rolls() {
        let mut s = Telemetry::default();
        let mut v = serde_json::json!({"downloadTotal":100,"uploadTotal":0,"connections":[{"id":"a","download":10,"upload":0,"chains":["DIRECT"],"rule":"DomainSuffix","rulePayload":"example.invalid"}]});
        s.connections(&v, 1893456000);
        s.connections(&v, 1893456001);
        assert_eq!(s.routes, [0, 10]);
        assert_eq!(s.hits["DomainSuffix:example.invalid"], 1);
        v["downloadTotal"] = 120.into();
        v["connections"][0]["download"] = 30.into();
        s.connections(&v, 1893456002);
        assert_eq!(s.down, 120);
        assert_eq!(s.routes, [0, 30]);
        s.probe("node", true, 1893456002);
        s.probe("node", false, 1893456003);
        assert_eq!(s.uptime("node"), Some(50.));
        s.reject(1893456003);
        assert_eq!(s.blocked, 1);
        s.roll(1893542700);
        assert_eq!(s.blocked, 0);
        assert_eq!(s.down, 0);
        assert_eq!(s.uptime("node"), None);
    }
}
