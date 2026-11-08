// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use reqwest::RequestBuilder;

#[derive(Clone)]
pub struct Api {
    http: reqwest::Client,
    secret: String,
    base: String,
}

impl Api {
    pub fn new(secret: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(20))
            .pool_max_idle_per_host(1)
            .build()
            .expect("reqwest client");
        Self {
            http,
            secret: secret.into(),
            base: format!("http://{}", crate::config::controller_addr()),
        }
    }

    /// Opens a controller websocket such as `/traffic` or `/logs?level=info`.
    pub async fn websocket(
        &self,
        path: &str,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        String,
    > {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://{}{path}", crate::config::controller_addr())
            .into_client_request()
            .map_err(|err| err.to_string())?;
        if let Some(secret) = self.bearer() {
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {secret}")
                    .parse()
                    .map_err(|err| format!("authorization header: {err}"))?,
            );
        }
        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|err| err.to_string())?;
        Ok(socket)
    }

    pub fn bearer(&self) -> Option<&str> {
        if self.secret.is_empty() {
            None
        } else {
            Some(self.secret.as_str())
        }
    }

    fn authorized(&self, builder: RequestBuilder) -> RequestBuilder {
        if self.secret.is_empty() {
            builder
        } else {
            builder.bearer_auth(&self.secret)
        }
    }

    pub async fn version(&self) -> Result<String, String> {
        let response = self
            .authorized(self.http.get(format!("{}/version", self.base)))
            .send()
            .await
            .map_err(|err| err.to_string())?;
        let status = response.status();
        let body = response.text().await.map_err(|err| err.to_string())?;
        if !status.is_success() {
            return Err(format!("GET /version -> {status}: {body}"));
        }
        Ok(body)
    }

    pub async fn mode(&self) -> Result<String, String> {
        let value = self.get_json("/configs").await?;
        value
            .get("mode")
            .and_then(|item| item.as_str())
            .map(str::to_string)
            .ok_or_else(|| "configs response has no mode".to_string())
    }

    pub async fn set_mode(&self, mode: &str) -> Result<(), String> {
        self.send_json(
            self.http.patch(format!("{}/configs", self.base)),
            serde_json::json!({ "mode": mode }),
        )
        .await
    }
    pub async fn patch_configs(&self, value: serde_json::Value) -> Result<(), String> {
        self.send_json(self.http.patch(format!("{}/configs", self.base)), value)
            .await
    }
    pub async fn reload_path(&self, path: &std::path::Path) -> Result<(), String> {
        self.send_json(
            self.http.put(format!("{}/configs?force=true", self.base)),
            serde_json::json!({"path":path.to_str().ok_or("配置路径无效")?}),
        )
        .await
    }
    pub async fn close_connection(&self, id: &str) -> Result<(), String> {
        self.send(self.http.delete(format!(
            "{}/connections/{}",
            self.base,
            encode_component(id)
        )))
        .await?;
        Ok(())
    }

    pub async fn proxies(&self) -> Result<serde_json::Value, String> {
        self.get_json("/proxies").await
    }

    pub async fn select(&self, group: &str, node: &str) -> Result<(), String> {
        let url = format!("{}/proxies/{}", self.base, encode_component(group));
        self.send_json(self.http.put(url), serde_json::json!({ "name": node }))
            .await
    }

    pub async fn log_level(&self) -> Result<String, String> {
        let value = self.get_json("/configs").await?;
        value
            .get("log-level")
            .and_then(|item| item.as_str())
            .map(str::to_string)
            .ok_or_else(|| "configs response has no log-level".to_string())
    }

    pub async fn set_log_level(&self, level: &str) -> Result<(), String> {
        self.send_json(
            self.http.patch(format!("{}/configs", self.base)),
            serde_json::json!({ "log-level": level }),
        )
        .await
    }

    pub async fn delay(&self, name: &str) -> Result<u32, String> {
        let fixture_target = crate::config::app_config_dir()
            .ok()
            .filter(|dir| dir.join("purr-validation-fixture").exists())
            .and_then(|_| std::env::var("PURR_VALIDATION_PROBE_URL").ok());
        let target = encode_component(
            fixture_target
                .as_deref()
                .unwrap_or("https://www.gstatic.com/generate_204"),
        );
        // 冷启动时首个握手（如 Reality）常超过 2.5 秒：先 2.5 秒测，没回应再用 5 秒重试一次，
        // 避免刚启动就把能用的节点标成「超时 · 丢 100%」。
        let mut last = String::new();
        for timeout in [2500u32, 5000] {
            let url = format!(
                "{}/proxies/{}/delay?timeout={timeout}&url={target}",
                self.base,
                encode_component(name)
            );
            match self.delay_once(&url).await {
                Ok(d) => return Ok(d),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    async fn delay_once(&self, url: &str) -> Result<u32, String> {
        let url = url.to_string();
        let value = self
            .send(self.http.get(url))
            .await?
            .json::<serde_json::Value>()
            .await
            .map_err(|err| err.to_string())?;
        value
            .get("delay")
            .and_then(|item| item.as_u64())
            .and_then(|delay| u32::try_from(delay).ok())
            .ok_or_else(|| "delay response has no delay".to_string())
    }

    /// `/proxies/{name}/delay?url=<url>&timeout=<ms>` for an arbitrary target URL.
    pub async fn delay_url(
        &self,
        name: &str,
        target: &str,
        timeout_ms: u32,
    ) -> Result<u32, String> {
        let url = format!(
            "{}/proxies/{}/delay?timeout={timeout_ms}&url={}",
            self.base,
            encode_component(name),
            encode_component(target)
        );
        let value = self
            .send(self.http.get(url))
            .await?
            .json::<serde_json::Value>()
            .await
            .map_err(|err| err.to_string())?;
        value
            .get("delay")
            .and_then(|item| item.as_u64())
            .and_then(|delay| u32::try_from(delay).ok())
            .filter(|d| *d > 0)
            .ok_or_else(|| "delay response has no delay".to_string())
    }

    pub async fn connections(&self) -> Result<serde_json::Value, String> {
        self.get_json("/connections").await
    }

    pub async fn rules(&self) -> Result<serde_json::Value, String> {
        self.get_json("/rules").await
    }

    pub async fn close_connections(&self) -> Result<(), String> {
        self.send(self.http.delete(format!("{}/connections", self.base)))
            .await?;
        Ok(())
    }

    pub async fn reload(&self) -> Result<(), String> {
        let url = format!("{}/configs?force=true", self.base);
        self.send_json(
            self.http.put(url),
            serde_json::json!({ "path": "", "payload": "" }),
        )
        .await
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let response = self
            .send(self.http.get(format!("{}{path}", self.base)))
            .await?;
        response.json().await.map_err(|err| err.to_string())
    }

    async fn send_json(
        &self,
        builder: RequestBuilder,
        body: serde_json::Value,
    ) -> Result<(), String> {
        self.send(builder.json(&body)).await?;
        Ok(())
    }

    async fn send(&self, builder: RequestBuilder) -> Result<reqwest::Response, String> {
        let response = self
            .authorized(builder)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("{status}: {body}"));
        }
        Ok(response)
    }

    pub async fn wait_ready(&self) -> Result<(), String> {
        let mut last = String::from("no response");
        for _ in 0..100 {
            match self.version().await {
                Ok(_) => return Ok(()),
                Err(err) => last = err,
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Err(format!("external controller not ready: {last}"))
    }
}

fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
