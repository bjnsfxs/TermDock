use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::broadcast;

use crate::{models::now_rfc3339, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/events", get(ws_events))
}

async fn ws_events(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| events_loop(socket, state))
}

async fn events_loop(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut events_rx = state.process.subscribe_events();

    if ws_tx
        .send(Message::Text(
            json!({
                "type": "hello",
                "daemon_version": env!("CARGO_PKG_VERSION"),
            })
            .to_string()
            .into(),
        ))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            event = events_rx.recv() => {
                match event {
                    Ok(event) => {
                        match serde_json::to_string(&event) {
                            Ok(text) => {
                                if ws_tx.send(Message::Text(text.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => {
                                tracing::warn!(error = %err, "failed to serialize process event");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        let notice = json!({
                            "type": "notice",
                            "level": "warn",
                            "message": format!("events lagged; dropped {skipped} frame(s)"),
                            "ts": now_rfc3339(),
                        })
                        .to_string();
                        if ws_tx.send(Message::Text(notice.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            frame = ws_rx.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(text.as_ref()) {
                            if v.get("type").and_then(|x| x.as_str()) == Some("ping") {
                                if ws_tx
                                    .send(Message::Text(json!({"type":"pong"}).to_string().into()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if ws_tx.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Err(err)) => {
                        tracing::warn!(error = %err, "events websocket receive failed");
                        break;
                    }
                }
            }
        }
    }
}
