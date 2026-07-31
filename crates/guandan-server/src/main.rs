//! Guandan WebSocket game server.

mod room;
mod settings;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use guandan_protocol::{decode_client, encode_server, ServerMessage};
use settings::GameSettings;
use state::AppState;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "guandan-server", about = "掼蛋 WebSocket 服务器")]
struct Args {
    /// Listen address
    #[arg(long, default_value = "0.0.0.0:9100")]
    bind: String,
    /// Turn time limit in seconds (auto-pass / auto-lead). Standard: 30.
    #[arg(long, default_value_t = 30, env = "GUANDAN_TURN_SECS")]
    turn_timeout_secs: u64,
    /// Seconds to hold a play on screen before the next seat acts. Standard: 3.
    #[arg(long, default_value_t = 3, env = "GUANDAN_REVEAL_SECS")]
    play_reveal_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "guandan_server=info".into()),
        )
        .init();

    let args = Args::parse();
    let settings = GameSettings {
        turn_timeout: Duration::from_secs(args.turn_timeout_secs.max(5)),
        play_reveal: Duration::from_secs(args.play_reveal_secs),
    };
    let state = Arc::new(AppState::new(settings));
    let listener = TcpListener::bind(&args.bind).await?;
    info!(
        "掼蛋服务器监听 {}  ·  turn={}s  reveal={}s",
        args.bind, args.turn_timeout_secs, args.play_reveal_secs
    );

    // Game tick: turn timeouts + bots (respecting play reveal)
    let tick_state = Arc::clone(&state);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            tick_state.tick_game().await;
        }
    });

    loop {
        let (stream, addr) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(state, stream, addr).await {
                warn!("连接 {} 结束: {e}", addr);
            }
        });
    }
}

async fn handle_connection(
    state: Arc<AppState>,
    stream: TcpStream,
    addr: SocketAddr,
) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let session_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    state.register(session_id, player_id, tx.clone()).await;

    let hello = encode_server(&ServerMessage::Connected {
        session_id,
        player_id,
    })?;
    sink.send(Message::Text(hello.into())).await?;

    let online = state.online_count().await;
    let _ = tx.send(encode_server(&ServerMessage::OnlineCount {
        count: online,
    })?);

    // Writer task
    let write_task = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Reader loop
    let read_result = async {
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            match msg {
                Message::Text(text) => match decode_client(&text) {
                    Ok(cm) => {
                        if let Err(e) = state.handle(session_id, cm).await {
                            let err = encode_server(&ServerMessage::Error {
                                message: e.to_string(),
                            })?;
                            let _ = state.send_to(session_id, err).await;
                        }
                    }
                    Err(e) => {
                        let err = encode_server(&ServerMessage::Error {
                            message: format!("协议错误: {e}"),
                        })?;
                        let _ = state.send_to(session_id, err).await;
                    }
                },
                Message::Ping(data) => {
                    // tungstenite handles most pings; respond with app pong too
                    let _ = data;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    write_task.abort();
    state.unregister(session_id).await;
    info!("客户端断开 {}", addr);
    read_result
}
