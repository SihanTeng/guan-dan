//! WebSocket client connection.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use guandan_protocol::{decode_server, encode_client, ClientMessage, ServerMessage};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub struct NetHandle {
    tx: mpsc::UnboundedSender<ClientMessage>,
}

impl NetHandle {
    pub async fn connect(url: &str) -> Result<(Self, mpsc::UnboundedReceiver<ServerMessage>)> {
        let (ws, _) = connect_async(url)
            .await
            .with_context(|| format!("连接服务器失败: {url}"))?;
        let (mut sink, mut stream) = ws.split();

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let (in_tx, in_rx) = mpsc::unbounded_channel::<ServerMessage>();

        // Writer
        tokio::spawn(async move {
            while let Some(msg) = out_rx.recv().await {
                if let Ok(text) = encode_client(&msg) {
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Reader
        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(sm) = decode_server(&text) {
                        if in_tx.send(sm).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok((Self { tx: out_tx }, in_rx))
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.tx.send(msg);
    }
}
