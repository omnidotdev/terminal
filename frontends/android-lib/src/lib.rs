mod renderer;
mod terminal;

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
use terminal::TerminalGrid;
use tungstenite::Message;

static TERMINAL_STATE: Mutex<Option<TerminalState>> = Mutex::new(None);

/// Messages sent from JNI to the WebSocket thread.
enum WsCommand {
    /// Send raw bytes to the PTY (keyboard input).
    Input(Vec<u8>),
    /// Resize the remote PTY (includes JSON with session_id).
    Resize(String),
    /// Disconnect and shut down.
    Disconnect,
}

struct TerminalState {
    sugarloaf: Sugarloaf<'static>,
    rt_id: usize,
    grid: TerminalGrid,
    parser: copa::Parser,
    total_cols: usize,
    total_rows: usize,
    surface_width: f32,
    surface_height: f32,
    scale: f32,
    /// Whether font dimensions have been confirmed (non-zero from sugarloaf).
    dims_confirmed: bool,
    /// Send commands to the WebSocket thread.
    ws_tx: Option<mpsc::Sender<WsCommand>>,
    /// Receive PTY output from the WebSocket thread.
    ws_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// Session UUID (set after "created" response).
    session_id: Option<[u8; 16]>,
    /// Whether content needs re-rendering.
    dirty: bool,
    /// Whether we're connected to a server.
    connected: bool,
    /// Error message to display on status screen.
    error_msg: Option<String>,
}

impl TerminalState {
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
                    self.grid.resize(cols, rows);
                    self.send_resize(cols, rows);
                    self.dirty = true;
                }
            }
        }

        // Drain PTY output from WebSocket — collect first to avoid borrow issues
        let mut incoming: Vec<Vec<u8>> = Vec::new();
        if let Some(ref rx) = self.ws_rx {
            while let Ok(data) = rx.try_recv() {
                incoming.push(data);
            }
        }
        for data in incoming {
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

        if !self.dirty && self.connected {
            return;
        }

        if self.connected && self.session_id.is_some() {
            // Real terminal: render the grid
            renderer::render_grid(&mut self.sugarloaf, &self.grid, self.rt_id);
        } else {
            // Not connected: show status screen
            self.render_status_screen();
        }

        self.sugarloaf
            .set_objects(vec![Object::RichText(RichText {
                id: self.rt_id,
                position: [0.0, 0.0],
                lines: None,
            })]);
        self.sugarloaf.render();
        self.dirty = false;
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

        content.add_text(" omni", green);
        content.add_text("@terminal", white);
        content.new_line();
        content.new_line();

        if let Some(ref err) = self.error_msg {
            let red = FragmentStyle {
                color: [1.0, 0.3, 0.3, 1.0],
                ..FragmentStyle::default()
            };
            content.add_text(&format!("  Error: {err}"), red);
            content.new_line();
            content.add_text("  Press back to try again", dim);
        } else if self.connected {
            content.add_text("  Connecting to server...", dim);
        } else {
            content.add_text("  Not connected", dim);
            content.new_line();
            content.add_text("  Press back to enter server URL", dim);
        }

        content.new_line();
        content.build();
    }

    fn send_input(&self, data: &[u8]) {
        if let (Some(ref tx), Some(ref sid)) = (&self.ws_tx, &self.session_id) {
            let mut frame = sid.to_vec();
            frame.extend_from_slice(data);
            let _ = tx.send(WsCommand::Input(frame));
        }
    }

    fn send_resize(&self, cols: usize, rows: usize) {
        if let (Some(ref tx), Some(ref sid)) = (&self.ws_tx, &self.session_id) {
            let uuid = uuid::Uuid::from_bytes(*sid);
            let msg = format!(
                r#"{{"type":"resize","session_id":"{uuid}","cols":{cols},"rows":{rows}}}"#
            );
            let _ = tx.send(WsCommand::Resize(msg));
        }
    }
}

/// Spawn a WebSocket client thread that connects to the server.
fn spawn_ws_thread(
    ws_url: String,
    cols: usize,
    rows: usize,
) -> (mpsc::Sender<WsCommand>, mpsc::Receiver<Vec<u8>>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WsCommand>();
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
    cmd_rx: &mpsc::Receiver<WsCommand>,
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
            Ok(WsCommand::Input(data)) => {
                if ws.send(Message::Binary(data.into())).is_err() {
                    log::error!("WebSocket send failed");
                    break;
                }
            }
            Ok(WsCommand::Resize(json)) => {
                if ws.send(Message::Text(json.into())).is_err() {
                    break;
                }
            }
            Ok(WsCommand::Disconnect) => {
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

    let cols = (width / cell_w).floor().max(1.0) as usize;
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

    let mut state = TerminalState {
        sugarloaf,
        rt_id,
        grid: TerminalGrid::new(cols, rows),
        parser: copa::Parser::new(),
        total_cols: cols,
        total_rows: rows,
        surface_width: width as f32,
        surface_height: height as f32,
        scale,
        dims_confirmed,
        ws_tx: None,
        ws_rx: None,
        session_id: None,
        dirty: true,
        connected: false,
        error_msg: None,
    };

    state.render_content();

    let mut global = TERMINAL_STATE.lock().unwrap();
    *global = Some(state);
}

/// Connect to a WebSocket server URL.
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

    let mut state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        // Disconnect existing connection
        if let Some(ref tx) = s.ws_tx {
            let _ = tx.send(WsCommand::Disconnect);
        }

        let (cmd_tx, out_rx) = spawn_ws_thread(url_str, s.total_cols, s.total_rows);
        s.ws_tx = Some(cmd_tx);
        s.ws_rx = Some(out_rx);
        s.session_id = None;
        s.connected = true;
        s.dirty = true;
        s.render_content();
    }
}

/// Render a frame — polls WebSocket output and re-renders if dirty.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_render(
    _env: JNIEnv,
    _class: JClass,
) {
    let mut state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        s.render_content();
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
    let mut state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        s.sugarloaf.resize(width as u32, height as u32);
        s.sugarloaf.rescale(scale);
        s.surface_width = width as f32;
        s.surface_height = height as f32;
        s.scale = scale;

        let (cols, rows) =
            calc_grid(width as f32, height as f32, scale, &mut s.sugarloaf, &s.rt_id);
        if cols != s.total_cols || rows != s.total_rows {
            s.total_cols = cols;
            s.total_rows = rows;
            s.grid.resize(cols, rows);
            s.send_resize(cols, rows);
        }
        s.dirty = true;
        s.render_content();
    }
}

/// Send a text string (from soft keyboard IME).
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

    let mut state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        s.send_input(input.as_bytes());
    }
}

/// Send a special key by code.
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

    let state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref s) = *state {
        s.send_input(bytes);
    }
}

/// Adjust font size. 0=reset, 1=decrease, 2=increase.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_setFontAction(
    _env: JNIEnv,
    _class: JClass,
    action: jint,
) {
    let mut state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        s.sugarloaf
            .set_rich_text_font_size_based_on_action(&s.rt_id, action as u8);
        s.dirty = true;
        s.render_content();
    }
}

/// Scroll by lines (no-op until scrollback is implemented).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_scroll(
    _env: JNIEnv,
    _class: JClass,
    _lines: jint,
) {
}

/// Clean up native resources.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_omnidotdev_terminal_NativeTerminal_destroy(
    _env: JNIEnv,
    _class: JClass,
) {
    log::info!("Destroying native terminal");
    let mut state = TERMINAL_STATE.lock().unwrap();
    if let Some(ref s) = *state {
        if let Some(ref tx) = s.ws_tx {
            let _ = tx.send(WsCommand::Disconnect);
        }
    }
    *state = None;
}
