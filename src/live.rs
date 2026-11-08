// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;

use crate::api::Api;
use crate::window::{self, LiveState};

pub async fn traffic_loop(api: Api, live: Arc<Mutex<LiveState>>) {
    loop {
        match connect(&api, "/traffic").await {
            Ok(mut socket) => {
                while let Some(message) = socket.next().await {
                    let Ok(message) = message else { break };
                    let Some(text) = message_text(message) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    let up = value.get("up").and_then(|item| item.as_u64()).unwrap_or(0);
                    let down = value
                        .get("down")
                        .and_then(|item| item.as_u64())
                        .unwrap_or(0);
                    window::set_speed(&live, up, down);
                }
                window::set_traffic_live(&live, false);
            }
            Err(err) => {
                window::set_traffic_live(&live, false);
                eprintln!("purr: traffic stream failed: {err}")
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn log_loop(api: Api, live: Arc<Mutex<LiveState>>) {
    loop {
        match connect(&api, "/logs?level=info").await {
            Ok(mut socket) => {
                while let Some(message) = socket.next().await {
                    let Ok(message) = message else { break };
                    let Some(text) = message_text(message) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    let kind = value
                        .get("type")
                        .and_then(|item| item.as_str())
                        .unwrap_or("info");
                    let payload = value
                        .get("payload")
                        .and_then(|item| item.as_str())
                        .unwrap_or(text.as_str());
                    if crate::present::is_reject_log(payload) {
                        window::count_reject(&live);
                    }
                    window::push_log(&live, kind, payload);
                }
            }
            Err(err) => eprintln!("purr: log stream failed: {err}"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn connect(
    api: &Api,
    path: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    String,
> {
    api.websocket(path).await
}

fn message_text(message: Message) -> Option<String> {
    match message {
        Message::Text(text) => Some(text.to_string()),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
        _ => None,
    }
}
