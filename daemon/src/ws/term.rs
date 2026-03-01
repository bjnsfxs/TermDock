use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::state::AppState;

const DEFAULT_TAIL_BYTES: usize = 8192;
const MAX_TAIL_BYTES: usize = 1024 * 1024;
const TERMINAL_OUTBOUND_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientControlMessage {
    #[serde(rename = "hello")]
    Hello {
        client_id: Option<String>,
        client_name: Option<String>,
    },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "tail")]
    Tail {
        #[serde(default = "default_tail_bytes")]
        bytes: usize,
    },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
}

fn default_tail_bytes() -> usize {
    DEFAULT_TAIL_BYTES
}

pub fn router() -> Router<AppState> {
    Router::new().route("/term/:id", get(ws_term))
}

async fn ws_term(
    Path(id): Path<Uuid>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| term_loop(socket, state, id))
}

async fn term_loop(mut socket: WebSocket, state: AppState, id: Uuid) {
    let attach = match state.process.attach_terminal(id).await {
        Ok(v) => v,
        Err(err) => {
            let _ = socket
                .send(ws_error(
                    "not_running",
                    format!("instance is not running: {err}"),
                ))
                .await;
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(TERMINAL_OUTBOUND_QUEUE_CAPACITY);

    let writer_task = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if ws_tx.send(frame).await.is_err() {
                break;
            }
        }
    });

    let _ = out_tx
        .send(Message::Text(
            json!({
                "type": "hello",
                "daemon_version": env!("CARGO_PKG_VERSION"),
            })
            .to_string()
            .into(),
        ))
        .await;
    let _ = out_tx
        .send(Message::Text(
            json!({
                "type": "status",
                "id": id,
                "status": attach.runtime.status,
                "pid": attach.runtime.pid,
                "backend": attach.backend,
                "clients_attached": attach.runtime.clients_attached,
            })
            .to_string()
            .into(),
        ))
        .await;

    let mut output_rx = attach.output_rx;
    let output_tx = out_tx.clone();
    let output_task = tokio::spawn(async move {
        loop {
            match output_rx.recv().await {
                Ok(chunk) => {
                    if output_tx.send(Message::Binary(chunk.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    let _ = output_tx
                        .send(ws_warning(
                            "output_lagged",
                            format!("output lagged; dropped {skipped} frame(s)"),
                        ))
                        .await;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(frame) = ws_rx.next().await {
        match frame {
            Ok(Message::Binary(payload)) => {
                if let Err(err) = state.process.write_input(id, payload.as_ref()).await {
                    let _ = out_tx.send(ws_error("write_failed", err.to_string())).await;
                    break;
                }
            }
            Ok(Message::Text(text)) => {
                if handle_control_message(&state, id, text.as_str(), &out_tx)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = out_tx.send(Message::Pong(payload)).await;
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(err) => {
                tracing::warn!(instance_id = %id, error = %err, "terminal websocket receive failed");
                break;
            }
        }
    }

    output_task.abort();
    drop(out_tx);
    let _ = writer_task.await;
    state.process.detach_terminal(id).await;
}

async fn handle_control_message(
    state: &AppState,
    id: Uuid,
    text: &str,
    out_tx: &mpsc::Sender<Message>,
) -> Result<(), ()> {
    let msg = match serde_json::from_str::<ClientControlMessage>(text) {
        Ok(v) => v,
        Err(err) => {
            let _ = out_tx
                .send(ws_error(
                    "invalid_message",
                    format!("invalid control message: {err}"),
                ))
                .await;
            return Ok(());
        }
    };

    match msg {
        ClientControlMessage::Hello {
            client_id,
            client_name,
        } => {
            let _ = out_tx
                .send(Message::Text(
                    json!({
                        "type": "hello",
                        "daemon_version": env!("CARGO_PKG_VERSION"),
                        "client_id": client_id,
                        "client_name": client_name,
                    })
                    .to_string()
                    .into(),
                ))
                .await;
            Ok(())
        }
        ClientControlMessage::Ping => {
            let _ = out_tx
                .send(Message::Text(json!({"type":"pong"}).to_string().into()))
                .await;
            Ok(())
        }
        ClientControlMessage::Tail { bytes } => {
            let requested = bytes.clamp(1, MAX_TAIL_BYTES);
            match state.process.tail_output(id, requested).await {
                Ok(tail) => {
                    let _ = out_tx
                        .send(Message::Text(
                            json!({
                                "type": "tail_begin",
                                "requested": requested,
                                "bytes": tail.data.len(),
                                "truncated": tail.truncated,
                            })
                            .to_string()
                            .into(),
                        ))
                        .await;
                    if !tail.data.is_empty() {
                        let _ = out_tx.send(Message::Binary(tail.data.into())).await;
                    }
                    Ok(())
                }
                Err(err) => {
                    let _ = out_tx.send(ws_error("tail_failed", err.to_string())).await;
                    Ok(())
                }
            }
        }
        ClientControlMessage::Resize { cols, rows } => {
            match state.process.resize_terminal(id, cols, rows).await {
                Ok(_) => Ok(()),
                Err(err) => {
                    let _ = out_tx
                        .send(ws_error("resize_failed", err.to_string()))
                        .await;
                    Ok(())
                }
            }
        }
    }
}

fn ws_error(code: &'static str, message: impl AsRef<str>) -> Message {
    Message::Text(
        json!({
            "type": "error",
            "code": code,
            "message": message.as_ref(),
        })
        .to_string()
        .into(),
    )
}

fn ws_warning(code: &'static str, message: impl AsRef<str>) -> Message {
    Message::Text(
        json!({
            "type": "warning",
            "code": code,
            "message": message.as_ref(),
        })
        .to_string()
        .into(),
    )
}
