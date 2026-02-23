use terminal_emulator::{TerminalGrid, render_grid};

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jfloat, jint};
use jni::JNIEnv;
use raw_window_handle::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use sugarloaf::layout::RootStyle;
use sugarloaf::{
    FragmentStyle, Object, RichText, Sugarloaf, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize,
};
use tungstenite::Message;

static TERMINAL_MANAGER: Mutex<Option<TerminalManager>> = Mutex::new(None);

/// Messages sent from JNI to the PTY/WebSocket thread.
enum PtyCommand {
    /// Send raw bytes to the PTY (keyboard input).
    Input(Vec<u8>),
    /// Resize the PTY.
    Resize(String),
    /// Disconnect and shut down.
    Disconnect,
}

struct Session {
    grid: TerminalGrid,
    parser: copa::Parser,
    /// Send commands to the WebSocket/PTY thread.
    ws_tx: Option<mpsc::Sender<PtyCommand>>,
    /// Receive PTY output from the WebSocket/PTY thread.
    ws_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// Session UUID (set after "created" response, remote only).
    session_id: Option<[u8; 16]>,
    /// Whether content needs re-rendering.
    dirty: bool,
    /// Whether we're connected to a server or local PTY.
    connected: bool,
    /// Error message to display on status screen.
    error_msg: Option<String>,
    /// Whether using a local PTY (vs remote WebSocket).
    local_mode: bool,
    /// Android files directory for local shell environment.
    files_dir: Option<String>,
    /// Tab display name.
    label: String,
    /// Whether the backing process/connection has exited.
    exited: bool,
}

impl Session {
    fn new(cols: usize, rows: usize, label: String) -> Self {
        Self {
            grid: TerminalGrid::new(cols, rows),
            parser: copa::Parser::new(),
            ws_tx: None,
            ws_rx: None,
            session_id: None,
            dirty: true,
            connected: false,
            error_msg: None,
            local_mode: false,
            files_dir: None,
            label,
            exited: false,
        }
    }

    /// Drain pending PTY/WebSocket output into the grid.
    fn drain_output(&mut self) {
        let mut incoming: Vec<Vec<u8>> = Vec::new();
        if let Some(ref rx) = self.ws_rx {
            loop {
                match rx.try_recv() {
                    Ok(data) => incoming.push(data),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.exited = true;
                        break;
                    }
                }
            }
        }
        for data in incoming {
            if self.local_mode {
                self.parser.advance(&mut self.grid, &data);
                self.dirty = true;
            } else {
                if let Ok(text) = std::str::from_utf8(&data) {
                    if text.starts_with('{') {
                        self.handle_control_message(text);
                        continue;
                    }
                }
                // Binary PTY output: first 16 bytes = session UUID
                if data.len() > 16 {
                    let pty_data = &data[16..];
                    self.parser.advance(&mut self.grid, pty_data);
                    self.dirty = true;
                }
            }
        }
    }

    fn handle_control_message(&mut self, text: &str) {
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(text) {
            let msg_type = msg.get("type").and_then(|v| v.as_str());
            match msg_type {
                Some("created") | Some("attached") => {
                    if let Some(sid_str) = msg.get("session_id").and_then(|v| v.as_str())
                    {
                        if let Ok(uuid) = uuid::Uuid::parse_str(sid_str) {
                            self.session_id = Some(*uuid.as_bytes());
                            log::info!("Session established: {sid_str}");
                            self.dirty = true;
                        }
                    }
                }
                Some("error") => {
                    let err = msg
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    log::error!("Server error: {err}");
                    self.error_msg = Some(err);
                    self.connected = false;
                    self.dirty = true;
                }
                _ => {}
            }
        }
    }

    fn send_input(&self, data: &[u8]) {
        if let Some(ref tx) = self.ws_tx {
            if self.local_mode {
                let _ = tx.send(PtyCommand::Input(data.to_vec()));
            } else if let Some(ref sid) = self.session_id {
                let mut frame = sid.to_vec();
                frame.extend_from_slice(data);
                let _ = tx.send(PtyCommand::Input(frame));
            }
        }
    }

    fn send_resize(&self, cols: usize, rows: usize) {
        if let Some(ref tx) = self.ws_tx {
            if self.local_mode {
                let msg = format!(r#"{{"cols":{cols},"rows":{rows}}}"#);
                let _ = tx.send(PtyCommand::Resize(msg));
            } else if let Some(ref sid) = self.session_id {
                let uuid = uuid::Uuid::from_bytes(*sid);
                let msg = format!(
                    r#"{{"type":"resize","session_id":"{uuid}","cols":{cols},"rows":{rows}}}"#
                );
                let _ = tx.send(PtyCommand::Resize(msg));
            }
        }
    }

    fn disconnect(&self) {
        if let Some(ref tx) = self.ws_tx {
            let _ = tx.send(PtyCommand::Disconnect);
        }
    }
}

struct TerminalManager {
    sugarloaf: Sugarloaf<'static>,
    rt_id: usize,
    sessions: Vec<Session>,
    active: usize,
    total_cols: usize,
    total_rows: usize,
    surface_width: f32,
    surface_height: f32,
    scale: f32,
    /// Whether font dimensions have been confirmed (non-zero from sugarloaf).
    dims_confirmed: bool,
    /// Monotonic counter for local shell labels (avoids duplicates on close/reopen).
    shell_counter: usize,
}

impl TerminalManager {
    fn active_session(&self) -> Option<&Session> {
        self.sessions.get(self.active)
    }

    fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(self.active)
    }

    /// Create a new local shell session and switch to it. Returns the new session index.
    fn create_local_session(&mut self, files_dir: &str, native_lib_dir: &str) -> usize {
        let label = self.next_shell_label();
        let mut session = Session::new(self.total_cols, self.total_rows, label);

        session.files_dir = Some(files_dir.to_string());
        let (cmd_tx, out_rx) =
            spawn_local_pty(files_dir, native_lib_dir, self.total_cols, self.total_rows);
        session.ws_tx = Some(cmd_tx);
        session.ws_rx = Some(out_rx);
        session.connected = true;
        session.local_mode = true;

        self.sessions.push(session);
        let idx = self.sessions.len() - 1;
        self.active = idx;
        idx
    }

    /// Create a new proot session and switch to it.
    fn create_proot_session(
        &mut self,
        files_dir: &str,
        rootfs_path: &str,
        proot_path: &str,
        native_lib_dir: &str,
    ) -> usize {
        self.shell_counter += 1;
        let label = if self.shell_counter == 1 {
            "Arch".to_string()
        } else {
            format!("Arch {}", self.shell_counter)
        };
        let mut session = Session::new(self.total_cols, self.total_rows, label);

        session.files_dir = Some(files_dir.to_string());
        let (cmd_tx, out_rx) = spawn_proot_pty(
            files_dir,
            rootfs_path,
            proot_path,
            native_lib_dir,
            self.total_cols,
            self.total_rows,
        );
        session.ws_tx = Some(cmd_tx);
        session.ws_rx = Some(out_rx);
        session.connected = true;
        session.local_mode = true;

        self.sessions.push(session);
        let idx = self.sessions.len() - 1;
        self.active = idx;
        idx
    }

    /// Create a new remote WebSocket session and switch to it. Returns the new session index.
    fn create_remote_session(&mut self, url: &str) -> usize {
        let label = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| "Remote".to_string());

        let mut session = Session::new(self.total_cols, self.total_rows, label);

        let (cmd_tx, out_rx) =
            spawn_ws_thread(url.to_string(), self.total_cols, self.total_rows);
        session.ws_tx = Some(cmd_tx);
        session.ws_rx = Some(out_rx);
        session.connected = true;

        self.sessions.push(session);
        let idx = self.sessions.len() - 1;
        self.active = idx;
        idx
    }

    /// Generate the next "Shell", "Shell 2", etc. label.
    fn next_shell_label(&mut self) -> String {
        self.shell_counter += 1;
        if self.shell_counter == 1 {
            "Shell".to_string()
        } else {
            format!("Shell {}", self.shell_counter)
        }
    }

    fn render_content(&mut self) {
        // Re-check grid size once font dimensions become available
        if !self.dims_confirmed {
            let dims = self.sugarloaf.get_rich_text_dimensions(&self.rt_id);
            if dims.width > 0.0 {
                self.dims_confirmed = true;
                let (cols, rows) = calc_grid(
                    self.surface_width,
                    self.surface_height,
                    self.scale,
                    &mut self.sugarloaf,
                    &self.rt_id,
                );
                if cols != self.total_cols || rows != self.total_rows {
                    log::info!(
                        "Font loaded — resizing grid: {}x{} -> {cols}x{rows}",
                        self.total_cols,
                        self.total_rows
                    );
                    self.total_cols = cols;
                    self.total_rows = rows;
                    for session in &mut self.sessions {
                        session.grid.resize(cols, rows);
                        session.send_resize(cols, rows);
                        session.dirty = true;
                    }
                }
            }
        }

        // Drain output from all sessions (background tabs stay up to date)
        for session in &mut self.sessions {
            session.drain_output();
        }

        // Render only the active session
        let needs_render = if let Some(session) = self.sessions.get(self.active) {
            session.dirty || !session.connected
        } else {
            true
        };

        if !needs_render {
            return;
        }

        if let Some(session) = self.sessions.get(self.active) {
            if session.connected && (session.local_mode || session.session_id.is_some()) {
                render_grid(&mut self.sugarloaf, &session.grid, self.rt_id);
            } else {
                self.render_status_screen();
            }
        } else {
            self.render_status_screen();
        }

        let pad_px = PADDING_DP * self.scale;
        self.sugarloaf
            .set_objects(vec![Object::RichText(RichText {
                id: self.rt_id,
                position: [pad_px, 0.0],
                lines: None,
            })]);
        self.sugarloaf.render();

        if let Some(session) = self.sessions.get_mut(self.active) {
            session.dirty = false;
        }
    }

    fn render_status_screen(&mut self) {
        let green = FragmentStyle {
            color: [0.0, 0.85, 0.4, 1.0],
            ..FragmentStyle::default()
        };
        let white = FragmentStyle {
            color: [0.9, 0.9, 0.9, 1.0],
            ..FragmentStyle::default()
        };
        let dim = FragmentStyle {
            color: [0.5, 0.5, 0.5, 1.0],
            ..FragmentStyle::default()
        };

        let content = self.sugarloaf.content();
        content.sel(self.rt_id).clear();

        content.add_text("omni", green);
        content.add_text("@terminal", white);
        content.new_line();
        content.new_line();

        if let Some(session) = self.sessions.get(self.active) {
            if let Some(ref err) = session.error_msg {
                let red = FragmentStyle {
                    color: [1.0, 0.3, 0.3, 1.0],
                    ..FragmentStyle::default()
                };
                let msg = format!("Error: {err}");
                for line in wrap_text(&msg, self.total_cols) {
                    content.add_text(&line, red);
                    content.new_line();
                }
                content.add_text("Press back to try again", dim);
            } else if session.connected {
                content.add_text("Connecting to server...", dim);
            } else {
                content.add_text("Not connected", dim);
                content.new_line();
                content.add_text("Press back to enter server URL", dim);
            }
        } else {
            content.add_text("No active session", dim);
        }

        content.new_line();
        content.build();
    }
}

/// Spawn a WebSocket client thread that connects to the server.
fn spawn_ws_thread(
    ws_url: String,
    cols: usize,
    rows: usize,
) -> (mpsc::Sender<PtyCommand>, mpsc::Receiver<Vec<u8>>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<PtyCommand>();
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();

    thread::Builder::new()
        .name("ws-client".into())
        .spawn(move || {
            ws_thread_main(&ws_url, cols, rows, &cmd_rx, &out_tx);
        })
        .expect("Failed to spawn WebSocket thread");

    (cmd_tx, out_rx)
}

fn ws_thread_main(
    ws_url: &str,
    cols: usize,
    rows: usize,
    cmd_rx: &mpsc::Receiver<PtyCommand>,
    out_tx: &mpsc::Sender<Vec<u8>>,
) {
    log::info!("WebSocket connecting to {ws_url}");

    // Parse the URL to extract host:port for manual TCP connect with timeout
    let parsed = match url::Url::parse(ws_url) {
        Ok(u) => u,
        Err(e) => {
            log::error!("Invalid URL {ws_url}: {e}");
            let _ = out_tx.send(
                br#"{"type":"error","message":"Invalid server URL"}"#.to_vec(),
            );
            return;
        }
    };
    let host = parsed.host_str().unwrap_or("localhost").to_string();
    let default_port = if parsed.scheme() == "wss" { 443 } else { 80 };
    let port = parsed.port().unwrap_or(default_port);
    let addr = format!("{host}:{port}");

    log::info!("Resolving {addr}");

    // Resolve DNS first, then connect with timeout
    use std::net::ToSocketAddrs;
    let sock_addr = match addr.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(a) => a,
            None => {
                log::error!("No addresses found for {addr}");
                let _ = out_tx.send(
                    format!(r#"{{"type":"error","message":"Cannot resolve {host}"}}"#)
                        .into_bytes(),
                );
                return;
            }
        },
        Err(e) => {
            log::error!("DNS resolution failed for {addr}: {e}");
            let _ = out_tx.send(
                format!(r#"{{"type":"error","message":"Cannot resolve {host}: {e}"}}"#)
                    .into_bytes(),
            );
            return;
        }
    };

    log::info!("Connecting to {sock_addr}");

    let tcp_stream = match std::net::TcpStream::connect_timeout(
        &sock_addr,
        std::time::Duration::from_secs(5),
    ) {
        Ok(s) => s,
        Err(e) => {
            log::error!("TCP connect to {addr} failed: {e}");
            let _ = out_tx.send(
                format!(r#"{{"type":"error","message":"Connection failed: {e}"}}"#)
                    .into_bytes(),
            );
            return;
        }
    };

    // Upgrade to WebSocket, wrapping with TLS for wss:// URLs
    let use_tls = parsed.scheme() == "wss";

    macro_rules! ws_handshake {
        ($stream:expr) => {
            match tungstenite::client(parsed.as_str(), $stream) {
                Ok((ws, _response)) => ws,
                Err(e) => {
                    log::error!("WebSocket handshake failed for {ws_url}: {e}");
                    let _ = out_tx.send(
                        br#"{"type":"error","message":"WebSocket handshake failed"}"#.to_vec(),
                    );
                    return;
                }
            }
        };
    }

    if use_tls {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(AcceptAnyCert))
            .with_no_client_auth();
        let connector = rustls::StreamOwned::new(
            rustls::ClientConnection::new(
                std::sync::Arc::new(tls_config),
                host.try_into().unwrap_or_else(|_| "localhost".try_into().unwrap()),
            )
            .expect("failed to create TLS connection"),
            tcp_stream,
        );
        let mut ws = ws_handshake!(connector);
        let _ = ws.get_ref().sock.set_nonblocking(true);
        ws_event_loop(&mut ws, cols, rows, cmd_rx, out_tx);
    } else {
        let mut ws = ws_handshake!(tcp_stream);
        let _ = ws.get_ref().set_nonblocking(true);
        ws_event_loop(&mut ws, cols, rows, cmd_rx, out_tx);
    };

    log::info!("WebSocket thread exiting");
}

fn ws_event_loop<S: std::io::Read + std::io::Write>(
    ws: &mut tungstenite::WebSocket<S>,
    cols: usize,
    rows: usize,
    cmd_rx: &mpsc::Receiver<PtyCommand>,
    out_tx: &mpsc::Sender<Vec<u8>>,
) {
    log::info!("WebSocket connected");

    // Send create session request
    let create_msg = format!(r#"{{"type":"create","cols":{cols},"rows":{rows}}}"#);
    if ws.send(Message::Text(create_msg.into())).is_err() {
        log::error!("Failed to send create message");
        return;
    }

    loop {
        // Check for commands from JNI
        match cmd_rx.try_recv() {
            Ok(PtyCommand::Input(data)) => {
                if ws.send(Message::Binary(data.into())).is_err() {
                    log::error!("WebSocket send failed");
                    break;
                }
            }
            Ok(PtyCommand::Resize(json)) => {
                if ws.send(Message::Text(json.into())).is_err() {
                    break;
                }
            }
            Ok(PtyCommand::Disconnect) => {
                let _ = ws.close(None);
                break;
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // Read from WebSocket
        match ws.read() {
            Ok(Message::Binary(data)) => {
                let _ = out_tx.send(data.to_vec());
            }
            Ok(Message::Text(text)) => {
                let _ = out_tx.send(text.as_bytes().to_vec());
            }
            Ok(Message::Close(_)) => {
                log::info!("WebSocket closed by server");
                break;
            }
            Ok(_) => {} // Ping/Pong handled internally
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                // No data available yet — sleep briefly to avoid busy-loop
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                log::error!("WebSocket error: {e}");
                break;
            }
        }
    }
}

/// Accept any TLS certificate (needed for self-signed dev certs)
#[derive(Debug)]
struct AcceptAnyCert;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Word-wrap text to fit within `cols` columns.
fn wrap_text(text: &str, cols: usize) -> Vec<String> {
    if cols == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut line = String::new();

    for word in text.split(' ') {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + 1 + word.len() <= cols {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(line);
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }

    lines
}

/// Create local shell directories under `files_dir`.
fn ensure_local_dirs(files_dir: &str) {
    use std::ffi::CString;

    let dirs = [
        format!("{files_dir}/home"),
        format!("{files_dir}/usr"),
        format!("{files_dir}/usr/bin"),
        format!("{files_dir}/usr/tmp"),
        format!("{files_dir}/usr/etc"),
        format!("{files_dir}/usr/share/terminfo"),
    ];

    for dir in &dirs {
        if let Ok(c_path) = CString::new(dir.as_str()) {
            unsafe {
                libc::mkdir(c_path.as_ptr(), 0o755);
            }
        }
    }
}

/// Spawn a local PTY shell process.
fn spawn_local_pty(
    files_dir: &str,
    native_lib_dir: &str,
    cols: usize,
    rows: usize,
) -> (mpsc::Sender<PtyCommand>, mpsc::Receiver<Vec<u8>>) {
    use nix::pty::openpty;
    use nix::unistd::{dup2, execve, fork, setsid, ForkResult};
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let home = format!("{files_dir}/home");
    let prefix = format!("{files_dir}/usr");

    ensure_local_dirs(files_dir);

    let (cmd_tx, cmd_rx) = mpsc::channel::<PtyCommand>();
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();

    let pty = openpty(None, None).expect("openpty failed");
    let master_fd = pty.master;
    let slave_fd = pty.slave;

    // Set initial terminal size
    set_winsize(master_fd.as_raw_fd(), cols as u16, rows as u16);

    // Clone strings for the child process (pre-fork)
    let home_c = home.clone();
    let prefix_c = prefix.clone();
    let native_lib_dir_c = native_lib_dir.to_string();

    match unsafe { fork() } {
        #[allow(unreachable_code)]
        Ok(ForkResult::Child) => {
            // Child process: set up slave as controlling terminal
            drop(master_fd);

            setsid().expect("setsid failed");

            // Set slave as controlling terminal
            unsafe {
                libc::ioctl(slave_fd.as_raw_fd(), libc::TIOCSCTTY, 0);
            }

            dup2(slave_fd.as_raw_fd(), 0).expect("dup2 stdin failed");
            dup2(slave_fd.as_raw_fd(), 1).expect("dup2 stdout failed");
            dup2(slave_fd.as_raw_fd(), 2).expect("dup2 stderr failed");

            if slave_fd.as_raw_fd() > 2 {
                drop(slave_fd);
            }

            // chdir to $HOME
            if let Ok(c_home) = CString::new(home_c.as_str()) {
                unsafe {
                    libc::chdir(c_home.as_ptr());
                }
            }

            // Build env with bootstrap path first
            let make_env = |path_val: &str| -> Vec<CString> {
                [
                    format!("HOME={home_c}"),
                    path_val.to_string(),
                    format!("PREFIX={prefix_c}"),
                    format!("TMPDIR={prefix_c}/tmp"),
                    "TERM=xterm-256color".to_string(),
                    "COLORTERM=truecolor".to_string(),
                    "LANG=en_US.UTF-8".to_string(),
                    format!("TERMINFO={prefix_c}/share/terminfo"),
                    format!("ENV={home_c}/.profile"),
                ]
                .iter()
                .filter_map(|s| CString::new(s.as_str()).ok())
                .collect()
            };

            // Try busybox from native lib dir first (always executable,
            // not affected by noexec restrictions on app data dirs)
            let bootstrap_path = format!("PATH={prefix_c}/bin:/system/bin");
            let bootstrap_env = make_env(&bootstrap_path);
            let bootstrap_refs: Vec<&CString> = bootstrap_env.iter().collect();

            {
                let busybox_path = format!("{native_lib_dir_c}/libbusybox.so");
                if std::path::Path::new(&busybox_path).exists() {
                    if let Ok(shell) = CString::new(busybox_path.as_str()) {
                        let argv0 = CString::new("-ash").unwrap();
                        let argv = [argv0];
                        let _ = execve(&shell, &argv, &bootstrap_refs);
                    }
                }
            }

            // Try bootstrap shells from prefix (may fail on noexec mounts)
            for (path, arg0) in [
                (format!("{prefix_c}/bin/bash"), "-bash"),
                (format!("{prefix_c}/bin/ash"), "-ash"),
            ] {
                if !std::path::Path::new(&path).exists() {
                    continue;
                }
                if let Ok(shell) = CString::new(path.as_str()) {
                    let argv0 = CString::new(arg0).unwrap();
                    let argv = [argv0];
                    let _ = execve(&shell, &argv, &bootstrap_refs);
                }
            }

            // Bootstrap shells failed (noexec); fall back to system shell
            // with /system/bin first so system commands aren't shadowed
            let fallback_path = format!("PATH=/system/bin:{prefix_c}/bin");
            let fallback_env = make_env(&fallback_path);
            let fallback_refs: Vec<&CString> = fallback_env.iter().collect();

            let sys_shell = CString::new("/system/bin/sh").unwrap();
            let sys_argv0 = CString::new("sh").unwrap();
            let sys_argv = [sys_argv0];
            let _ = execve(&sys_shell, &sys_argv, &fallback_refs);

            // All candidates failed
            eprintln!("fatal: no usable shell found");
            unsafe { libc::_exit(127) };
        }
        Ok(ForkResult::Parent { child }) => {
            drop(slave_fd);

            // Set master to non-blocking
            unsafe {
                let flags = libc::fcntl(master_fd.as_raw_fd(), libc::F_GETFL);
                libc::fcntl(
                    master_fd.as_raw_fd(),
                    libc::F_SETFL,
                    flags | libc::O_NONBLOCK,
                );
            }

            let master_raw = master_fd.as_raw_fd();
            // Prevent OwnedFd from closing on drop in this thread — the PTY thread owns it
            std::mem::forget(master_fd);

            thread::Builder::new()
                .name("pty-local".into())
                .spawn(move || {
                    let master = unsafe { OwnedFd::from_raw_fd(master_raw) };
                    pty_thread_main(master, child, &cmd_rx, &out_tx);
                })
                .expect("Failed to spawn PTY thread");
        }
        Err(e) => {
            log::error!("fork failed: {e}");
        }
    }

    (cmd_tx, out_rx)
}

/// Spawn a local PTY running through proot with the Arch Linux rootfs.
fn spawn_proot_pty(
    files_dir: &str,
    rootfs_path: &str,
    proot_path: &str,
    native_lib_dir: &str,
    cols: usize,
    rows: usize,
) -> (mpsc::Sender<PtyCommand>, mpsc::Receiver<Vec<u8>>) {
    use nix::pty::openpty;
    use nix::unistd::{dup2, execve, fork, setsid, ForkResult};
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    ensure_local_dirs(files_dir);

    let (cmd_tx, cmd_rx) = mpsc::channel::<PtyCommand>();
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();

    let pty = openpty(None, None).expect("openpty failed");
    let master_fd = pty.master;
    let slave_fd = pty.slave;

    set_winsize(master_fd.as_raw_fd(), cols as u16, rows as u16);

    let proot_path = proot_path.to_string();
    let rootfs_path = rootfs_path.to_string();
    let files_dir = files_dir.to_string();
    let native_lib_dir = native_lib_dir.to_string();

    log::info!("spawn_proot_pty: proot={proot_path} rootfs={rootfs_path}");

    match unsafe { fork() } {
        #[allow(unreachable_code)]
        Ok(ForkResult::Child) => {
            drop(master_fd);

            setsid().expect("setsid failed");

            unsafe {
                libc::ioctl(slave_fd.as_raw_fd(), libc::TIOCSCTTY, 0);
            }

            dup2(slave_fd.as_raw_fd(), 0).expect("dup2 stdin failed");
            dup2(slave_fd.as_raw_fd(), 1).expect("dup2 stdout failed");
            dup2(slave_fd.as_raw_fd(), 2).expect("dup2 stderr failed");

            let slave_raw = slave_fd.as_raw_fd();
            if slave_raw > 2 {
                drop(slave_fd);
            }

            // Close all inherited FDs > 2 (Android graphics FDs, etc.)
            unsafe {
                for fd in 3..256 {
                    if fd != slave_raw {
                        libc::close(fd);
                    }
                }
            }

            // Create libtalloc.so.2 symlink so the dynamic linker can find it
            // (Termux's proot links against libtalloc.so.2 but we ship libtalloc.so)
            let lib_dir = format!("{files_dir}/usr/lib");
            let _ = std::fs::create_dir_all(&lib_dir);
            let symlink_path = format!("{lib_dir}/libtalloc.so.2");
            let target_path = format!("{native_lib_dir}/libtalloc.so");
            let _ = std::fs::remove_file(&symlink_path);
            let _ = std::os::unix::fs::symlink(&target_path, &symlink_path);

            let proot = CString::new(proot_path.as_str()).unwrap();
            let rootfs_arg = format!("--rootfs={rootfs_path}");

            let argv_strs = [
                "proot",
                &rootfs_arg,
                "--bind=/dev",
                "--bind=/proc",
                "--bind=/sys",
                "--bind=/sdcard",
                "-0",
                "-w",
                "/root",
                "/usr/bin/bash",
                "-l",
            ];
            let argv: Vec<CString> = argv_strs
                .iter()
                .filter_map(|s| CString::new(*s).ok())
                .collect();
            let argv_refs: Vec<&CString> = argv.iter().collect();

            let tmp_dir = format!("{files_dir}/usr/tmp");
            let loader_path = format!("{native_lib_dir}/libproot-loader.so");
            let env_vars: Vec<CString> = [
                "HOME=/root".to_string(),
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
                "TERM=xterm-256color".to_string(),
                "COLORTERM=truecolor".to_string(),
                "LANG=en_US.UTF-8".to_string(),
                format!("PROOT_TMP_DIR={tmp_dir}"),
                format!("PROOT_LOADER={loader_path}"),
                format!("LD_LIBRARY_PATH={lib_dir}:{native_lib_dir}"),
            ]
            .iter()
            .filter_map(|s| CString::new(s.as_str()).ok())
            .collect();

            let env_refs: Vec<&CString> = env_vars.iter().collect();
            match execve(&proot, &argv_refs, &env_refs) {
                Ok(_) => unreachable!(),
                Err(e) => {
                    let msg = format!("execve failed: {e}\n");
                    let _ = nix::unistd::write(std::io::stderr(), msg.as_bytes());
                    unsafe { libc::_exit(1) };
                }
            }
        }
        Ok(ForkResult::Parent { child }) => {
            drop(slave_fd);

            unsafe {
                let flags = libc::fcntl(master_fd.as_raw_fd(), libc::F_GETFL);
                libc::fcntl(
                    master_fd.as_raw_fd(),
                    libc::F_SETFL,
                    flags | libc::O_NONBLOCK,
                );
            }

            let master_raw = master_fd.as_raw_fd();
            std::mem::forget(master_fd);

            thread::Builder::new()
                .name("pty-proot".into())
                .spawn(move || {
                    let master = unsafe { OwnedFd::from_raw_fd(master_raw) };
                    pty_thread_main(master, child, &cmd_rx, &out_tx);
                })
                .expect("Failed to spawn proot PTY thread");
        }
        Err(e) => {
            log::error!("fork failed: {e}");
        }
    }

    (cmd_tx, out_rx)
}

/// Set terminal window size via ioctl.
fn set_winsize(fd: i32, cols: u16, rows: u16) {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(fd, libc::TIOCSWINSZ, &ws);
    }
}

/// PTY thread main loop: shuttle data between master fd and channels.
fn pty_thread_main(
    master: std::os::fd::OwnedFd,
    child: nix::unistd::Pid,
    cmd_rx: &mpsc::Receiver<PtyCommand>,
    out_tx: &mpsc::Sender<Vec<u8>>,
) {
    use nix::sys::signal::{kill, Signal};
    use nix::sys::wait::{waitpid, WaitPidFlag};
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};

    let fd = master.as_raw_fd();
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    // Prevent double-close: File will close the fd, we must not drop OwnedFd
    std::mem::forget(master);

    let mut buf = [0u8; 4096];

    log::info!("PTY thread started, child pid={child}");

    loop {
        // Check for commands
        match cmd_rx.try_recv() {
            Ok(PtyCommand::Input(data)) => {
                let _ = file.write_all(&data);
            }
            Ok(PtyCommand::Resize(json)) => {
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&json) {
                    let cols = msg.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as u16;
                    let rows = msg.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as u16;
                    set_winsize(fd, cols, rows);
                    let _ = kill(child, Signal::SIGWINCH);
                }
            }
            Ok(PtyCommand::Disconnect) => {
                let _ = kill(child, Signal::SIGHUP);
                break;
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // Read from master fd
        match Read::read(&mut file, &mut buf) {
            Ok(0) => break, // EOF — shell exited
            Ok(n) => {
                if out_tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                log::error!("PTY read error: {e}");
                break;
            }
        }

        // Check if child has exited
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => {
                log::error!("Shell process exited with code {code}");
                // Drain any remaining output before exiting
                loop {
                    match Read::read(&mut file, &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let output = String::from_utf8_lossy(&buf[..n]);
                            log::error!("Shell final output: {output}");
                            let _ = out_tx.send(buf[..n].to_vec());
                        }
                    }
                }
                break;
            }
            Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                log::error!("Shell process killed by signal {sig}");
                break;
            }
            _ => {}
        }
    }

    log::info!("PTY thread exiting");
}

/// Horizontal padding in density-independent pixels (applied on each side).
const PADDING_DP: f32 = 6.0;

/// Calculate grid columns and rows from surface dimensions.
fn calc_grid(
    width: f32,
    height: f32,
    scale: f32,
    sugarloaf: &mut Sugarloaf,
    rt_id: &usize,
) -> (usize, usize) {
    let dims = sugarloaf.get_rich_text_dimensions(rt_id);
    log::info!(
        "calc_grid: surface={width}x{height} scale={scale} cell={}x{}",
        dims.width,
        dims.height
    );

    // dims are already in physical pixels (font shaped at scaled_font_size)
    let cell_w = if dims.width > 0.0 {
        dims.width
    } else {
        // Font not yet loaded — estimate: font_size * scale * 0.6
        18.0 * 0.6 * scale
    };
    let cell_h = if dims.height > 0.0 {
        dims.height
    } else {
        18.0 * 1.2 * scale
    };

    // Subtract horizontal padding from available width
    let usable_width = (width - 2.0 * PADDING_DP * scale).max(cell_w);

    let cols = (usable_width / cell_w).floor().max(1.0) as usize;
    let rows = (height / cell_h).floor().max(1.0) as usize;

    log::info!("calc_grid: result={cols}x{rows} cell_w={cell_w} cell_h={cell_h}");
    (cols, rows)
}

// --- JNI Functions ---

/// Initialize sugarloaf with an Android Surface.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_init(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
    width: jint,
    height: jint,
    scale: jfloat,
) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("OmniTerminal"),
    );
    log::info!("Initializing native terminal: {width}x{height} scale={scale}");

    let a_native_window = unsafe {
        let native_window = ndk::native_window::NativeWindow::from_surface(
            env.get_raw(),
            surface.as_raw(),
        );
        match native_window {
            Some(w) => w,
            None => {
                log::error!("Failed to get ANativeWindow from Surface");
                return;
            }
        }
    };

    let ptr = a_native_window.ptr();

    let window_handle =
        AndroidNdkWindowHandle::new(NonNull::new(ptr.as_ptr().cast()).unwrap());
    let display_handle = AndroidDisplayHandle::new();

    let sugarloaf_window = SugarloafWindow {
        handle: RawWindowHandle::AndroidNdk(window_handle),
        display: RawDisplayHandle::Android(display_handle),
        size: SugarloafWindowSize {
            width: width as f32,
            height: height as f32,
        },
        scale: scale as f32,
    };

    let layout = RootStyle {
        font_size: 18.0,
        line_height: 1.2,
        scale_factor: scale as f32,
    };

    let renderer = SugarloafRenderer {
        backend: wgpu::Backends::VULKAN,
        ..SugarloafRenderer::default()
    };

    let font_library = sugarloaf::font::FontLibrary::default();

    let result = Sugarloaf::new(sugarloaf_window, renderer, &font_library, layout);
    let mut sugarloaf = match result {
        Ok(instance) => {
            log::info!("Sugarloaf initialized successfully");
            instance
        }
        Err(e) => {
            log::error!("Failed to create sugarloaf: {e:?}");
            return;
        }
    };

    sugarloaf.set_background_color(Some(wgpu::Color {
        r: 0.05,
        g: 0.05,
        b: 0.1,
        a: 1.0,
    }));

    let rt_id = sugarloaf.create_rich_text();

    // Check if font dims are available yet
    let dims = sugarloaf.get_rich_text_dimensions(&rt_id);
    let dims_confirmed = dims.width > 0.0;

    let (cols, rows) =
        calc_grid(width as f32, height as f32, scale, &mut sugarloaf, &rt_id);

    log::info!("Grid: {cols}x{rows} dims_confirmed={dims_confirmed}");

    let mut mgr = TerminalManager {
        sugarloaf,
        rt_id,
        sessions: Vec::new(),
        active: 0,
        total_cols: cols,
        total_rows: rows,
        surface_width: width as f32,
        surface_height: height as f32,
        scale,
        dims_confirmed,
        shell_counter: 0,
    };

    mgr.render_content();

    let mut global = TERMINAL_MANAGER.lock().unwrap();
    *global = Some(mgr);
}

/// Connect to a WebSocket server URL (creates a new remote session).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_connect(
    mut env: JNIEnv,
    _class: JClass,
    url: JString,
) {
    let Ok(url_str) = env.get_string(&url) else {
        return;
    };
    let url_str: String = url_str.into();

    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.create_remote_session(&url_str);
        m.render_content();
    }
}

/// Connect to a local PTY shell (creates a new local session).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_connectLocal(
    mut env: JNIEnv,
    _class: JClass,
    files_dir: JString,
    native_lib_dir: JString,
) {
    let Ok(files_dir_jstr) = env.get_string(&files_dir) else {
        return;
    };
    let files_dir_str: String = files_dir_jstr.into();

    let Ok(native_lib_jstr) = env.get_string(&native_lib_dir) else {
        return;
    };
    let native_lib_str: String = native_lib_jstr.into();

    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.create_local_session(&files_dir_str, &native_lib_str);
        m.render_content();
    }
}

/// Connect to a local PTY through proot (creates a new proot session).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_connectLocalProot(
    mut env: JNIEnv,
    _class: JClass,
    files_dir: JString,
    rootfs_path: JString,
    proot_path: JString,
    native_lib_dir: JString,
) {
    let Ok(files_dir_jstr) = env.get_string(&files_dir) else {
        return;
    };
    let files_dir_str: String = files_dir_jstr.into();

    let Ok(rootfs_jstr) = env.get_string(&rootfs_path) else {
        return;
    };
    let rootfs_str: String = rootfs_jstr.into();

    let Ok(proot_jstr) = env.get_string(&proot_path) else {
        return;
    };
    let proot_str: String = proot_jstr.into();

    let Ok(native_lib_jstr) = env.get_string(&native_lib_dir) else {
        return;
    };
    let native_lib_str: String = native_lib_jstr.into();

    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.create_proot_session(&files_dir_str, &rootfs_str, &proot_str, &native_lib_str);
        m.render_content();
    }
}

/// Render a frame — polls PTY output and re-renders if dirty.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_render(
    _env: JNIEnv,
    _class: JClass,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.render_content();
    }
}

/// Handle surface resize.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_resize(
    _env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
    scale: jfloat,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.sugarloaf.resize(width as u32, height as u32);
        m.sugarloaf.rescale(scale);
        m.surface_width = width as f32;
        m.surface_height = height as f32;
        m.scale = scale;

        let (cols, rows) =
            calc_grid(width as f32, height as f32, scale, &mut m.sugarloaf, &m.rt_id);
        if cols != m.total_cols || rows != m.total_rows {
            m.total_cols = cols;
            m.total_rows = rows;
            for session in &mut m.sessions {
                session.grid.resize(cols, rows);
                session.send_resize(cols, rows);
            }
        }
        if let Some(session) = m.sessions.get_mut(m.active) {
            session.dirty = true;
        }
        m.render_content();
    }
}

/// Send a text string (from soft keyboard IME) to the active session.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_sendKey(
    mut env: JNIEnv,
    _class: JClass,
    text: JString,
) {
    let Ok(input) = env.get_string(&text) else {
        return;
    };
    let input: String = input.into();
    if input.is_empty() {
        return;
    }

    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        if let Some(session) = m.active_session() {
            session.send_input(input.as_bytes());
        }
        // Snap to bottom on user input
        if let Some(session) = m.active_session_mut() {
            session.grid.scroll_to_bottom();
        }
    }
}

/// Send a special key by code to the active session.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_sendSpecialKey(
    _env: JNIEnv,
    _class: JClass,
    key_code: jint,
) {
    let bytes: &[u8] = match key_code {
        1 => b"\r",           // Enter
        2 => &[0x7f],         // Backspace
        3 => b"\t",           // Tab
        4 => &[0x1b],         // Escape
        10 => b"\x1b[A",      // Arrow Up
        11 => b"\x1b[B",      // Arrow Down
        12 => b"\x1b[D",      // Arrow Left
        13 => b"\x1b[C",      // Arrow Right
        _ => return,
    };

    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        if let Some(session) = m.active_session() {
            session.send_input(bytes);
        }
        // Snap to bottom on user input
        if let Some(session) = m.active_session_mut() {
            session.grid.scroll_to_bottom();
        }
    }
}

/// Set the font size to an exact value (in CSS px).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_setFontSize(
    _env: JNIEnv,
    _class: JClass,
    size: jfloat,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.sugarloaf.set_rich_text_font_size(&m.rt_id, size);

        // Recalculate grid dimensions
        m.dims_confirmed = false;
        if let Some(session) = m.sessions.get_mut(m.active) {
            session.dirty = true;
        }
        m.render_content();
    }
}

/// Get the current font size.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getFontSize(
    _env: JNIEnv,
    _class: JClass,
) -> jfloat {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        return m.sugarloaf.rich_text_layout(&m.rt_id).font_size;
    }
    18.0
}

/// Adjust font size. 0=reset, 1=decrease, 2=increase.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_setFontAction(
    _env: JNIEnv,
    _class: JClass,
    action: jint,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.sugarloaf
            .set_rich_text_font_size_based_on_action(&m.rt_id, action as u8);
        if let Some(session) = m.sessions.get_mut(m.active) {
            session.dirty = true;
        }
        m.render_content();
    }
}

/// Scroll the viewport by the given number of lines.
/// Positive = scroll up (into history), negative = scroll down (toward live output).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_scroll(
    _env: JNIEnv,
    _class: JClass,
    lines: jint,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        if let Some(session) = m.active_session_mut() {
            session.grid.scroll_display(lines);
            session.dirty = true;
        }
    }
}

/// Get the current scroll offset (0 = at bottom/live).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getScrollOffset(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        if let Some(session) = m.active_session() {
            return session.grid.display_offset as jint;
        }
    }
    0
}

/// Get the maximum scroll offset (total scrollback lines).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getScrollMax(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        if let Some(session) = m.active_session() {
            return session.grid.scrollback_len() as jint;
        }
    }
    0
}

/// Switch to the session at the given index.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_switchSession(
    _env: JNIEnv,
    _class: JClass,
    index: jint,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        let idx = index as usize;
        if idx < m.sessions.len() {
            m.active = idx;
            if let Some(session) = m.sessions.get_mut(idx) {
                session.dirty = true;
            }
        }
    }
}

/// Close the session at the given index. Returns the number of remaining sessions.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_closeSession(
    _env: JNIEnv,
    _class: JClass,
    index: jint,
) -> jint {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        let idx = index as usize;
        if idx < m.sessions.len() {
            m.sessions[idx].disconnect();
            m.sessions.remove(idx);

            // Adjust active index. If active == idx and idx < new len,
            // active now points to the next session (which slid into the
            // removed slot) — this is the desired behavior.
            if m.sessions.is_empty() {
                m.active = 0;
            } else if m.active >= m.sessions.len() {
                m.active = m.sessions.len() - 1;
            } else if m.active > idx {
                m.active -= 1;
            }

            if let Some(session) = m.sessions.get_mut(m.active) {
                session.dirty = true;
            }
        }
        m.sessions.len() as jint
    } else {
        0
    }
}

/// Get the total number of sessions.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getSessionCount(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        m.sessions.len() as jint
    } else {
        0
    }
}

/// Get the active session index.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getActiveSession(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        m.active as jint
    } else {
        0
    }
}

/// Get the label for the session at the given index.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getSessionLabel<'a>(
    env: JNIEnv<'a>,
    _class: JClass<'a>,
    index: jint,
) -> JString<'a> {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    let label_owned = if let Some(ref m) = *mgr {
        m.sessions
            .get(index as usize)
            .map(|s| s.label.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    drop(mgr);

    env.new_string(&label_owned)
        .unwrap_or_else(|_| JObject::null().into())
}

/// Check whether the session at the given index is still alive (process has not exited).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_isSessionAlive(
    _env: JNIEnv,
    _class: JClass,
    index: jint,
) -> jboolean {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        if let Some(session) = m.sessions.get(index as usize) {
            return if session.exited { 0 } else { 1 };
        }
    }
    0
}

/// Begin a text selection at the given grid coordinates.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_selectionBegin(
    _env: JNIEnv,
    _class: JClass,
    col: jint,
    row: jint,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        if let Some(session) = m.active_session_mut() {
            session.grid.selection_begin(col as usize, row as usize);
        }
    }
}

/// Set the terminal background color (r, g, b as 0.0-1.0).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_setBackgroundColor(
    _env: JNIEnv,
    _class: JClass,
    r: jfloat,
    g: jfloat,
    b: jfloat,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.sugarloaf.set_background_color(Some(wgpu::Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: 1.0,
        }));
        if let Some(session) = m.sessions.get_mut(m.active) {
            session.dirty = true;
        }
    }
}

/// Update the end of the current text selection.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_selectionUpdate(
    _env: JNIEnv,
    _class: JClass,
    col: jint,
    row: jint,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        if let Some(session) = m.active_session_mut() {
            session.grid.selection_update(col as usize, row as usize);
        }
    }
}

/// Clear the current text selection.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_selectionClear(
    _env: JNIEnv,
    _class: JClass,
) {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        if let Some(session) = m.active_session_mut() {
            session.grid.selection_clear();
        }
    }
}

/// Get the currently selected text.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getSelectedText<'a>(
    env: JNIEnv<'a>,
    _class: JClass<'a>,
) -> JString<'a> {
    let mgr = TERMINAL_MANAGER.lock().unwrap();
    let text = if let Some(ref m) = *mgr {
        m.active_session()
            .map(|s| s.grid.selected_text())
            .unwrap_or_default()
    } else {
        String::new()
    };
    drop(mgr);
    env.new_string(&text)
        .unwrap_or_else(|_| JObject::null().into())
}

/// Get cell width in physical pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getCellWidth(
    _env: JNIEnv,
    _class: JClass,
) -> jfloat {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        let dims = m.sugarloaf.get_rich_text_dimensions(&m.rt_id);
        return dims.width;
    }
    0.0
}

/// Get cell height in physical pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_getCellHeight(
    _env: JNIEnv,
    _class: JClass,
) -> jfloat {
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        let dims = m.sugarloaf.get_rich_text_dimensions(&m.rt_id);
        return dims.height;
    }
    0.0
}

/// Clean up native resources.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_destroy(
    _env: JNIEnv,
    _class: JClass,
) {
    log::info!("Destroying native terminal");
    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        for session in &m.sessions {
            session.disconnect();
        }
    }
    *mgr = None;
}
