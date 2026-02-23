use terminal_emulator::{TerminalGrid, render_grid};

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jfloat, jint};
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
        }
    }

    /// Drain pending PTY/WebSocket output into the grid.
    fn drain_output(&mut self) {
        let mut incoming: Vec<Vec<u8>> = Vec::new();
        if let Some(ref rx) = self.ws_rx {
            while let Ok(data) = rx.try_recv() {
                incoming.push(data);
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
    fn create_local_session(&mut self, files_dir: &str) -> usize {
        let label = self.next_shell_label();
        let mut session = Session::new(self.total_cols, self.total_rows, label);

        session.files_dir = Some(files_dir.to_string());
        let (cmd_tx, out_rx) = spawn_local_pty(files_dir, self.total_cols, self.total_rows);
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
    let host = parsed.host_str().unwrap_or("localhost");
    let port = parsed.port().unwrap_or(80);
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

    // Upgrade TCP to WebSocket
    let Ok((mut ws, _response)) =
        tungstenite::client(parsed.as_str(), tcp_stream)
    else {
        log::error!("WebSocket handshake failed for {ws_url}");
        let _ = out_tx.send(
            br#"{"type":"error","message":"WebSocket handshake failed"}"#.to_vec(),
        );
        return;
    };

    log::info!("WebSocket connected");

    // Send create session request
    let create_msg = format!(r#"{{"type":"create","cols":{cols},"rows":{rows}}}"#);
    if ws.send(Message::Text(create_msg.into())).is_err() {
        log::error!("Failed to send create message");
        return;
    }

    // Set non-blocking so we can poll both WebSocket input and command channel
    let _ = ws.get_ref().set_nonblocking(true);

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

    log::info!("WebSocket thread exiting");
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

            // Detect best available shell
            let bash_path = format!("{prefix_c}/bin/bash");
            let ash_path = format!("{prefix_c}/bin/ash");

            let (shell_path, argv0) = if std::path::Path::new(&bash_path).exists() {
                (bash_path, "-bash")
            } else if std::path::Path::new(&ash_path).exists() {
                (ash_path, "-ash")
            } else {
                ("/system/bin/sh".to_string(), "sh")
            };

            // chdir to $HOME
            if let Ok(c_home) = CString::new(home_c.as_str()) {
                unsafe {
                    libc::chdir(c_home.as_ptr());
                }
            }

            let shell = CString::new(shell_path.as_str()).unwrap();
            let argv0 = CString::new(argv0).unwrap();
            let argv = [argv0];

            let path_val = format!("PATH={prefix_c}/bin:/system/bin");
            let env_vars: Vec<CString> = [
                format!("HOME={home_c}"),
                path_val,
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
            .collect();

            let env_refs: Vec<&CString> = env_vars.iter().collect();
            execve(&shell, &argv, &env_refs).expect("execve failed");
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
            Ok(nix::sys::wait::WaitStatus::Exited(_, _))
            | Ok(nix::sys::wait::WaitStatus::Signaled(_, _, _)) => {
                log::info!("Shell process exited");
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
) {
    let Ok(files_dir_jstr) = env.get_string(&files_dir) else {
        return;
    };
    let files_dir_str: String = files_dir_jstr.into();

    let mut mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref mut m) = *mgr {
        m.create_local_session(&files_dir_str);
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

    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        if let Some(session) = m.active_session() {
            session.send_input(input.as_bytes());
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

    let mgr = TERMINAL_MANAGER.lock().unwrap();
    if let Some(ref m) = *mgr {
        if let Some(session) = m.active_session() {
            session.send_input(bytes);
        }
    }
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
