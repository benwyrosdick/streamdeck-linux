//! WebSocket endpoint plugins and property inspectors connect to. The first
//! message on a socket registers it (`registerPlugin` with the plugin UUID,
//! or `registerPropertyInspector` with the action context); everything after
//! that is handed to the engine untouched.

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnKind {
    Plugin,
    PropertyInspector,
}

#[derive(Debug)]
pub enum SdkEvent {
    Registered { conn: u64, kind: ConnKind, id: String, tx: mpsc::UnboundedSender<String> },
    Message { conn: u64, value: Value },
    Closed { conn: u64 },
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Binds the SDK port on loopback. Port 0 picks a free port; the chosen port
/// is returned so plugins can be told about it.
pub async fn serve(port: u16, engine: mpsc::UnboundedSender<SdkEvent>) -> anyhow::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await.or(TcpListener::bind(("127.0.0.1", 0)).await)?;
    let bound = listener.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let engine = engine.clone();
            tokio::spawn(async move {
                stream.set_nodelay(true).ok();
                let ws = match tokio_tungstenite::accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        tracing::debug!("websocket handshake: {e}");
                        return;
                    }
                };
                let conn = NEXT.fetch_add(1, Ordering::Relaxed);
                let (mut sink, mut source) = ws.split();
                let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                let writer = tokio::spawn(async move {
                    while let Some(text) = rx.recv().await {
                        if sink.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                    sink.close().await.ok();
                });
                let mut registered = false;
                while let Some(Ok(msg)) = source.next().await {
                    let text = match msg {
                        Message::Text(t) => t.to_string(),
                        Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                        Message::Close(_) => break,
                        _ => continue,
                    };
                    let Ok(value) = serde_json::from_str::<Value>(&text) else {
                        tracing::debug!("conn {conn}: ignoring non-JSON message");
                        continue;
                    };
                    if !registered {
                        let event = value.get("event").and_then(|e| e.as_str()).unwrap_or("");
                        let id = value.get("uuid").and_then(|u| u.as_str()).unwrap_or("").to_string();
                        let kind = match event {
                            "registerPlugin" => ConnKind::Plugin,
                            "registerPropertyInspector" => ConnKind::PropertyInspector,
                            _ => continue,
                        };
                        registered = true;
                        engine.send(SdkEvent::Registered { conn, kind, id, tx: tx.clone() }).ok();
                        continue;
                    }
                    engine.send(SdkEvent::Message { conn, value }).ok();
                }
                if registered {
                    engine.send(SdkEvent::Closed { conn }).ok();
                }
                drop(tx);
                writer.abort();
            });
        }
    });
    Ok(bound)
}
