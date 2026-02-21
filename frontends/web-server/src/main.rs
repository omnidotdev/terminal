mod session;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use session::{SessionId, SessionManager};
use std::net::SocketAddr;
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    session_manager: SessionManager,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "web_server=info".into()),
        )
        .init();

    let state = AppState {
        session_manager: SessionManager::default(),
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("frontends/wasm"))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!("Omni Terminal web server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let manager = state.session_manager;

    // Track active session and its output receiver
    let mut active_session: Option<SessionId> = None;
    let mut output_rx: Option<mpsc::UnboundedReceiver<Vec<u8>>> = None;

    // Wait for initial control message to create or attach session
    loop {
        tokio::select! {
            // Forward PTY output to WebSocket
            Some(data) = async {
                if let Some(ref mut rx) = output_rx {
                    rx.recv().await
                } else {
                    // No active session yet — yield forever
                    std::future::pending::<Option<Vec<u8>>>().await
                }
            } => {
                if let Some(session_id) = active_session {
                    // Binary frame: 16 bytes session UUID + PTY output
                    let mut frame = session_id.as_bytes().to_vec();
                    frame.extend_from_slice(&data);
                    if ws_sender.send(Message::Binary(frame.into())).await.is_err() {
                        break;
                    }
                }
            }

            // Handle incoming WebSocket messages
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match handle_control_message(
                            &text,
                            &manager,
                            &mut active_session,
                            &mut output_rx,
                            &mut ws_sender,
                        ).await {
                            Ok(should_continue) => {
                                if !should_continue {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = ws_sender.send(Message::Text(
                                    serde_json::json!({
                                        "type": "error",
                                        "message": e
                                    }).to_string().into()
                                )).await;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        // Binary frame: first 16 bytes = session UUID, rest = PTY input
                        if data.len() > 16 {
                            let session_id = SessionId::from_slice(&data[..16]);
                            if let Ok(sid) = session_id {
                                if let Err(e) = manager.write_to_session(&sid, &data[16..]) {
                                    tracing::error!("Write error: {e}");
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    // Clean up active session on disconnect
    if let Some(session_id) = active_session {
        tracing::info!("WebSocket disconnected, closing session {session_id}");
        manager.close_session(&session_id);
    }
}

async fn handle_control_message(
    text: &str,
    manager: &SessionManager,
    active_session: &mut Option<SessionId>,
    output_rx: &mut Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    ws_sender: &mut (impl SinkExt<Message, Error = axum::Error> + Unpin),
) -> Result<bool, String> {
    let msg: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {e}"))?;

    let msg_type = msg
        .get("type")
        .and_then(|t| t.as_str())
        .ok_or("Missing 'type' field")?;

    match msg_type {
        "create" => {
            let cols = msg
                .get("cols")
                .and_then(|v| v.as_u64())
                .unwrap_or(80) as u16;
            let rows = msg
                .get("rows")
                .and_then(|v| v.as_u64())
                .unwrap_or(24) as u16;

            let (session_id, rx) = manager.create_session(cols, rows)?;
            *active_session = Some(session_id);
            *output_rx = Some(rx);

            let response = serde_json::json!({
                "type": "created",
                "session_id": session_id.to_string(),
            });

            let _ = ws_sender
                .send(Message::Text(response.to_string().into()))
                .await;

            Ok(true)
        }
        "resize" => {
            let session_id_str = msg
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing session_id")?;
            let session_id: SessionId = session_id_str
                .parse()
                .map_err(|_| "Invalid session_id")?;
            let cols = msg
                .get("cols")
                .and_then(|v| v.as_u64())
                .unwrap_or(80) as u16;
            let rows = msg
                .get("rows")
                .and_then(|v| v.as_u64())
                .unwrap_or(24) as u16;

            manager.resize_session(&session_id, cols, rows)?;
            Ok(true)
        }
        "close" => {
            let session_id_str = msg
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing session_id")?;
            let session_id: SessionId = session_id_str
                .parse()
                .map_err(|_| "Invalid session_id")?;

            manager.close_session(&session_id);
            *active_session = None;
            *output_rx = None;
            Ok(true)
        }
        _ => Err(format!("Unknown message type: {msg_type}")),
    }
}
