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
use std::collections::HashMap;
use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::sync::Arc;
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

    // Spawn reaper task to clean up stale disconnected sessions
    let reaper_manager = state.session_manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            reaper_manager.reap_stale_sessions(std::time::Duration::from_secs(60));
        }
    });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("frontends/wasm"))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let (cert_pem, key_pem) = match (
        std::env::var("TLS_CERT").ok(),
        std::env::var("TLS_KEY").ok(),
    ) {
        (Some(cert_path), Some(key_path)) => {
            tracing::info!("using provided TLS certificate");
            (
                std::fs::read(&cert_path).expect("failed to read TLS cert file"),
                std::fs::read(&key_path).expect("failed to read TLS key file"),
            )
        }
        _ => {
            tracing::info!("generating self-signed TLS certificate");
            let generated = rcgen::generate_simple_self_signed(vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
            ])
            .expect("failed to generate self-signed certificate");

            (
                generated.cert.pem().into_bytes(),
                generated.key_pair.serialize_pem().into_bytes(),
            )
        }
    };

    // Force HTTP/1.1 only — h2 ALPN negotiation breaks WebSocket upgrades
    let certs: Vec<_> = rustls_pemfile::certs(&mut &*cert_pem)
        .collect::<Result<_, _>>()
        .expect("invalid certificate PEM");
    let key = rustls_pemfile::private_key(&mut &*key_pem)
        .expect("invalid private key PEM")
        .expect("no private key found in PEM");
    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("invalid certificate/key pair");
    server_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let tls_config = RustlsConfig::from_config(Arc::new(server_config));

    tracing::info!("Omni Terminal web server listening on https://{addr}");
    axum_server::bind_rustls(addr, tls_config)
        .serve(app.into_make_service())
        .await
        .unwrap();
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

    // Merged output channel: all sessions' PTY output flows through here
    let (merged_tx, mut merged_rx) = mpsc::unbounded_channel::<(SessionId, Vec<u8>)>();

    // Track active sessions and their forwarding tasks
    let mut session_tasks: HashMap<SessionId, tokio::task::JoinHandle<()>> = HashMap::new();

    loop {
        tokio::select! {
            // Forward merged PTY output to WebSocket
            Some((session_id, data)) = merged_rx.recv() => {
                let mut frame = session_id.as_bytes().to_vec();
                frame.extend_from_slice(&data);
                if ws_sender.send(Message::Binary(frame.into())).await.is_err() {
                    break;
                }
            }

            // Handle incoming WebSocket messages
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match handle_control_message(
                            &text,
                            &manager,
                            &merged_tx,
                            &mut session_tasks,
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

    // Detach all sessions on disconnect, keeping PTYs alive for reconnection
    for (session_id, handle) in session_tasks {
        handle.abort();
        tracing::info!("WebSocket disconnected, detaching session {session_id}");
        manager.detach_session(&session_id);
    }
}

/// Forward a single session's PTY output into the merged channel
fn spawn_output_forwarder(
    session_id: SessionId,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    merged_tx: mpsc::UnboundedSender<(SessionId, Vec<u8>)>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if merged_tx.send((session_id, data)).is_err() {
                break;
            }
        }
    })
}

async fn handle_control_message(
    text: &str,
    manager: &SessionManager,
    merged_tx: &mpsc::UnboundedSender<(SessionId, Vec<u8>)>,
    session_tasks: &mut HashMap<SessionId, tokio::task::JoinHandle<()>>,
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

            let handle = spawn_output_forwarder(session_id, rx, merged_tx.clone());
            session_tasks.insert(session_id, handle);

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
        "attach" => {
            let session_id_str = msg
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing session_id")?;
            let session_id: SessionId = session_id_str
                .parse()
                .map_err(|_| "Invalid session_id")?;

            let (rx, buffered) = manager.attach_session(&session_id)?;

            let handle = spawn_output_forwarder(session_id, rx, merged_tx.clone());
            session_tasks.insert(session_id, handle);

            // Send buffered output first
            if !buffered.is_empty() {
                let mut frame = session_id.as_bytes().to_vec();
                frame.extend_from_slice(&buffered);
                let _ = ws_sender.send(Message::Binary(frame.into())).await;
            }

            let response = serde_json::json!({
                "type": "attached",
                "session_id": session_id.to_string(),
            });
            let _ = ws_sender
                .send(Message::Text(response.to_string().into()))
                .await;

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

            // Abort the forwarding task for this session
            if let Some(handle) = session_tasks.remove(&session_id) {
                handle.abort();
            }

            manager.close_session(&session_id);
            Ok(true)
        }
        _ => Err(format!("Unknown message type: {msg_type}")),
    }
}
