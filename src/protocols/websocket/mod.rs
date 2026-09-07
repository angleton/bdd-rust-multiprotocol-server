use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};

use crate::{protocols::protocol_response, AppState, Telemetry};

pub(crate) async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state.telemetry))
}

async fn handle_socket(mut socket: WebSocket, telemetry: Telemetry) {
    while let Some(result) = socket.recv().await {
        let message = match result {
            Ok(message) => message,
            Err(error) => {
                eprintln!("WebSocket receive failed: {error}");
                break;
            }
        };

        let started = std::time::Instant::now();
        match message {
            Message::Text(request) => {
                let response = protocol_response("WebSocket");
                let request_bytes = request.len() as u64;
                let response_bytes = response.len() as u64;
                let success = socket
                    .send(Message::Text(response.clone().into()))
                    .await
                    .is_ok();

                // Record each client action as soon as its response is sent. While
                // `cargo run` is active in a console, `/telemetry` exposes this
                // real-time WebSocket activity alongside the other protocols.
                telemetry.record(
                    "websocket",
                    success,
                    started.elapsed(),
                    request_bytes,
                    response_bytes,
                );

                if !success {
                    break;
                }
            }
            Message::Binary(request) => {
                telemetry.record(
                    "websocket",
                    false,
                    started.elapsed(),
                    request.len() as u64,
                    0,
                );
                let _ = socket
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: axum::extract::ws::close_code::UNSUPPORTED,
                        reason: "text messages are required".into(),
                    })))
                    .await;
                break;
            }
            Message::Ping(payload) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    break;
                }
            }
            Message::Pong(_) | Message::Close(_) => break,
        }
    }
}
