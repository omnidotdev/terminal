#![cfg(target_arch = "wasm32")]

use terminal_emulator::{render_grid, MouseMode, TerminalGrid};

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WebDisplayHandle, WebWindowHandle,
};
use std::cell::RefCell;
use std::rc::Rc;
use sugarloaf::layout::RootStyle;
use sugarloaf::{
    Object, RichText, Sugarloaf, SugarloafRenderer, SugarloafWindow, SugarloafWindowSize,
};
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, HtmlDivElement, HtmlElement, HtmlTextAreaElement};

/// Height of the tab bar in CSS pixels
const TAB_BAR_HEIGHT: u32 = 36;

fn get_or_create_canvas(container: &HtmlElement) -> (HtmlCanvasElement, u32) {
    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");

    if let Ok(Some(existing)) = container.query_selector("#terminal-canvas") {
        let canvas: HtmlCanvasElement = existing.unchecked_into();
        let id = 1u32;
        canvas
            .set_attribute("data-raw-handle", &id.to_string())
            .unwrap();
        return (canvas, id);
    }

    let canvas: HtmlCanvasElement =
        document.create_element("canvas").unwrap().unchecked_into();
    canvas.set_id("terminal-canvas");
    let id = 1u32;
    canvas
        .set_attribute("data-raw-handle", &id.to_string())
        .unwrap();

    canvas
        .set_attribute(
            "style",
            &format!(
                "width: 100%; height: calc(100% - {}px); display: block;",
                TAB_BAR_HEIGHT
            ),
        )
        .unwrap();

    container.append_child(&canvas).unwrap();

    let dpr = window.device_pixel_ratio();
    let width = (canvas.client_width() as f64 * dpr) as u32;
    let height = (canvas.client_height() as f64 * dpr) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    (canvas, id)
}

/// Create hidden textarea (IME target) and preedit overlay div
fn create_ime_elements(container: &HtmlElement) -> (HtmlTextAreaElement, HtmlDivElement) {
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");

    // Hidden textarea -- the OS sends composition events here
    let textarea: HtmlTextAreaElement = document
        .create_element("textarea")
        .unwrap()
        .unchecked_into();
    textarea.set_id("ime-input");
    textarea
        .set_attribute(
            "style",
            "width: 1px; height: 1px; opacity: 0; position: absolute; left: 0; top: 0; overflow: hidden; z-index: -1;",
        )
        .unwrap();
    textarea.set_attribute("autocapitalize", "off").unwrap();
    textarea.set_attribute("autocomplete", "off").unwrap();
    textarea.set_attribute("autocorrect", "off").unwrap();
    textarea.set_attribute("spellcheck", "false").unwrap();
    container.append_child(&textarea).unwrap();

    // Preedit overlay -- show the composition string during active IME input
    let overlay: HtmlDivElement =
        document.create_element("div").unwrap().unchecked_into();
    overlay.set_id("ime-overlay");
    overlay
        .set_attribute(
            "style",
            "position: absolute; display: none; color: white; background: rgba(30, 30, 30, 0.9); font-family: monospace; font-size: 16px; border-bottom: 2px solid white; pointer-events: none; white-space: pre; padding: 2px 4px; z-index: 1000;",
        )
        .unwrap();
    container.append_child(&overlay).unwrap();

    (textarea, overlay)
}

/// Shared state for the WebSocket connection, accessible by all handlers
struct WsState {
    ws: Option<web_sys::WebSocket>,
    backoff_ms: u32,
}

/// Shared state for mouse tracking across event handlers
#[derive(Debug)]
struct MouseState {
    last_col: usize,
    last_row: usize,
    buttons_down: u8,
}

/// Single terminal tab with its own session, grid, and parser
struct Tab {
    session_id: Option<[u8; 16]>,
    grid: TerminalGrid,
    parser: copa::Parser,
    title: String,
}

/// Manage multiple terminal tabs
struct TabManager {
    tabs: Vec<Tab>,
    active: usize,
}

impl TabManager {
    /// Create a new TabManager with one initial tab
    fn new(cols: usize, rows: usize) -> Self {
        let tab = Tab {
            session_id: None,
            grid: TerminalGrid::new(cols, rows),
            parser: copa::Parser::new(),
            title: "Tab 1".to_string(),
        };
        Self {
            tabs: vec![tab],
            active: 0,
        }
    }

    fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// Add a new tab, returning its index
    fn add_tab(&mut self, cols: usize, rows: usize) -> usize {
        let idx = self.tabs.len();
        let tab = Tab {
            session_id: None,
            grid: TerminalGrid::new(cols, rows),
            parser: copa::Parser::new(),
            title: format!("Tab {}", idx + 1),
        };
        self.tabs.push(tab);
        idx
    }

    /// Close tab at index, returning its session_id for cleanup.
    /// Returns None if this is the last tab (refuses to close).
    fn close_tab(&mut self, idx: usize) -> Option<[u8; 16]> {
        if self.tabs.len() <= 1 {
            return None;
        }
        if idx >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(idx);
        // Adjust active index
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if self.active > idx {
            self.active -= 1;
        }
        tab.session_id
    }

    fn switch_to(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = idx;
            // Mark new active tab dirty so it gets rendered
            self.tabs[self.active].grid.dirty = true;
        }
    }

    /// Route PTY output to the tab with the matching session_id
    fn route_output(&mut self, session_id: &[u8; 16], data: &[u8]) {
        for tab in &mut self.tabs {
            if tab.session_id.as_ref() == Some(session_id) {
                tab.parser.advance(&mut tab.grid, data);
                return;
            }
        }
    }

    fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}

/// Extract X11-style modifier bitmask from a browser mouse event
fn mouse_modifiers(event: &web_sys::MouseEvent) -> u8 {
    let mut mods = 0u8;
    if event.shift_key() {
        mods |= 4;
    }
    if event.alt_key() {
        mods |= 8;
    }
    if event.ctrl_key() {
        mods |= 16;
    }
    mods
}

/// Map browser button index to X11 button code
fn x11_button(browser_button: i16) -> u8 {
    match browser_button {
        0 => 0, // Left
        1 => 1, // Middle
        2 => 2, // Right
        _ => 0,
    }
}

/// Convert CSS pixel offset to terminal grid cell coordinates
fn pixel_to_cell(
    offset_x: i32,
    offset_y: i32,
    cell_width: f32,
    cell_height: f32,
) -> (usize, usize) {
    let dpr = web_sys::window().unwrap().device_pixel_ratio();
    let px_x = offset_x as f64 * dpr;
    let px_y = offset_y as f64 * dpr;
    let col = if cell_width > 0.0 {
        (px_x as f32 / cell_width).max(0.0) as usize
    } else {
        0
    };
    let row = if cell_height > 0.0 {
        (px_y as f32 / cell_height).max(0.0) as usize
    } else {
        0
    };
    (col, row)
}

/// Create the tab bar DOM element above the canvas
fn create_tab_bar(container: &HtmlElement) {
    let document = web_sys::window().unwrap().document().unwrap();

    let tab_bar: HtmlDivElement =
        document.create_element("div").unwrap().unchecked_into();
    tab_bar.set_id("tab-bar");
    tab_bar
        .set_attribute(
            "style",
            &format!(
                "display: flex; background: #1a1a2e; border-bottom: 1px solid #333; height: {}px; align-items: center; padding: 6px 0; gap: 4px; user-select: none; flex-shrink: 0;",
                TAB_BAR_HEIGHT
            ),
        )
        .unwrap();

    // Insert tab bar as first child of container
    let first_child = container.first_child();
    container
        .insert_before(&tab_bar, first_child.as_ref())
        .unwrap();
}

/// Rebuild the tab bar buttons from current TabManager state.
/// Captures `tabs` and `ws_state` to wire click handlers.
fn rebuild_tab_bar(tabs: &Rc<RefCell<TabManager>>, ws_state: &Rc<RefCell<WsState>>) {
    let document = web_sys::window().unwrap().document().unwrap();
    let Some(tab_bar) = document.get_element_by_id("tab-bar") else {
        return;
    };

    // Clear existing buttons
    tab_bar.set_inner_html("");

    let tabs_ref = tabs.borrow();
    let tab_count = tabs_ref.tab_count();
    let active = tabs_ref.active;

    for i in 0..tab_count {
        let title = &tabs_ref.tabs[i].title;
        let is_active = i == active;

        // Tab button container
        let tab_btn: HtmlDivElement =
            document.create_element("div").unwrap().unchecked_into();

        let bg = if is_active { "#2a2a4e" } else { "transparent" };
        tab_btn
            .set_attribute(
                "style",
                &format!(
                    "padding: 5px 8px; cursor: pointer; color: #ccc; font-family: monospace; font-size: 12px; border-radius: 4px; background: {}; display: flex; align-items: center; gap: 6px;",
                    bg
                ),
            )
            .unwrap();

        // Tab label span
        let label: web_sys::HtmlSpanElement =
            document.create_element("span").unwrap().unchecked_into();
        label.set_text_content(Some(title));

        // Click on label/tab to switch
        {
            let tabs = tabs.clone();
            let ws_state = ws_state.clone();
            let on_click = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
                move |event: web_sys::MouseEvent| {
                    event.stop_propagation();
                    tabs.borrow_mut().switch_to(i);
                    rebuild_tab_bar(&tabs, &ws_state);
                },
            );
            let target: &web_sys::EventTarget = label.as_ref();
            target
                .add_event_listener_with_callback(
                    "click",
                    on_click.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_click.forget();
        }

        tab_btn.append_child(&label).unwrap();

        // Close button (only if more than 1 tab)
        if tab_count > 1 {
            let close_btn: web_sys::HtmlSpanElement =
                document.create_element("span").unwrap().unchecked_into();
            close_btn.set_text_content(Some("\u{00d7}")); // multiplication sign as close icon
            close_btn
                .set_attribute(
                    "style",
                    "cursor: pointer; color: #888; font-size: 14px; line-height: 1; padding: 0 2px;",
                )
                .unwrap();

            let tabs = tabs.clone();
            let ws_state = ws_state.clone();
            let on_close = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
                move |event: web_sys::MouseEvent| {
                    event.stop_propagation();
                    let sid = tabs.borrow_mut().close_tab(i);
                    if let Some(sid) = sid {
                        // Send close message to server
                        let close_msg = format!(
                            r#"{{"type":"close","session_id":"{}"}}"#,
                            uuid::Uuid::from_bytes(sid)
                        );
                        let state = ws_state.borrow();
                        if let Some(ref ws) = state.ws {
                            if ws.ready_state() == web_sys::WebSocket::OPEN {
                                let _ = ws.send_with_str(&close_msg);
                            }
                        }
                        drop(state);
                    }
                    rebuild_tab_bar(&tabs, &ws_state);
                },
            );
            let target: &web_sys::EventTarget = close_btn.as_ref();
            target
                .add_event_listener_with_callback(
                    "click",
                    on_close.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_close.forget();

            tab_btn.append_child(&close_btn).unwrap();
        }

        tab_bar.append_child(&tab_btn).unwrap();
    }

    // "+" button to add a new tab
    let add_btn: HtmlDivElement =
        document.create_element("div").unwrap().unchecked_into();
    add_btn
        .set_attribute(
            "style",
            "padding: 5px 8px; cursor: pointer; color: #888; font-family: monospace; font-size: 14px; border-radius: 4px;",
        )
        .unwrap();
    add_btn.set_text_content(Some("+"));

    {
        let tabs = tabs.clone();
        let ws_state = ws_state.clone();
        let on_add = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
            move |_event: web_sys::MouseEvent| {
                // Grab dimensions from the active tab
                let (cols, rows) = {
                    let tabs_ref = tabs.borrow();
                    let active = tabs_ref.active_tab();
                    (active.grid.cols, active.grid.rows)
                };
                let new_idx = tabs.borrow_mut().add_tab(cols, rows);
                tabs.borrow_mut().switch_to(new_idx);

                // Send create message for the new tab
                let create_msg =
                    format!(r#"{{"type":"create","cols":{},"rows":{}}}"#, cols, rows);
                let state = ws_state.borrow();
                if let Some(ref ws) = state.ws {
                    if ws.ready_state() == web_sys::WebSocket::OPEN {
                        let _ = ws.send_with_str(&create_msg);
                    }
                }
                drop(state);

                rebuild_tab_bar(&tabs, &ws_state);
            },
        );
        let target: &web_sys::EventTarget = add_btn.as_ref();
        target
            .add_event_listener_with_callback("click", on_add.as_ref().unchecked_ref())
            .unwrap();
        on_add.forget();
    }

    tab_bar.append_child(&add_btn).unwrap();
}

/// Connect or reconnect the WebSocket with auto-reconnect on close/error
fn connect_ws(
    ws_state: &Rc<RefCell<WsState>>,
    tabs: &Rc<RefCell<TabManager>>,
    url: &Rc<String>,
) {
    let url = url.clone();
    let ws = web_sys::WebSocket::new(&url).expect("Failed to create WebSocket");
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // on_open -- reattach all tabs with existing sessions, create for tabs without
    {
        let ws_state = ws_state.clone();
        let tabs = tabs.clone();
        let on_open = Closure::<dyn FnMut()>::new(move || {
            ws_state.borrow_mut().backoff_ms = 0; // Reset backoff on successful connect

            let tabs_ref = tabs.borrow();
            let state = ws_state.borrow();

            for tab in &tabs_ref.tabs {
                if let Some(sid) = tab.session_id {
                    let attach_msg = format!(
                        r#"{{"type":"attach","session_id":"{}"}}"#,
                        uuid::Uuid::from_bytes(sid)
                    );
                    if let Some(ref ws) = state.ws {
                        let _ = ws.send_with_str(&attach_msg);
                    }
                } else {
                    let create_msg = format!(
                        r#"{{"type":"create","cols":{},"rows":{}}}"#,
                        tab.grid.cols, tab.grid.rows
                    );
                    if let Some(ref ws) = state.ws {
                        let _ = ws.send_with_str(&create_msg);
                    }
                }
            }
            log::info!(
                "WebSocket connected, reattaching/creating {} tab(s)",
                tabs_ref.tabs.len()
            );
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();
    }

    // on_message -- process PTY output
    {
        let ws_state = ws_state.clone();
        let tabs = tabs.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                // Text messages are control responses (JSON)
                if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
                    let text: String = text.into();
                    if let Ok(msg) = js_sys::JSON::parse(&text) {
                        let msg_type = js_sys::Reflect::get(&msg, &"type".into())
                            .ok()
                            .and_then(|v| v.as_string());
                        // New session -- assign to the first tab without a session_id
                        if msg_type.as_deref() == Some("created") {
                            if let Some(sid) =
                                js_sys::Reflect::get(&msg, &"session_id".into())
                                    .ok()
                                    .and_then(|v| v.as_string())
                            {
                                if let Ok(uuid) = uuid::Uuid::parse_str(&sid) {
                                    let mut tabs_ref = tabs.borrow_mut();
                                    let target_idx = tabs_ref
                                        .tabs
                                        .iter()
                                        .position(|t| t.session_id.is_none())
                                        .unwrap_or(tabs_ref.active);
                                    tabs_ref.tabs[target_idx].session_id =
                                        Some(*uuid.as_bytes());
                                    log::info!("Session created: {sid}");
                                }
                            }
                        }

                        // Reattached -- tab already has the correct session_id
                        if msg_type.as_deref() == Some("attached") {
                            if let Some(sid) =
                                js_sys::Reflect::get(&msg, &"session_id".into())
                                    .ok()
                                    .and_then(|v| v.as_string())
                            {
                                log::info!("Session reattached: {sid}");
                            }
                        }

                        // Attach failed -- clear stale session_id and create fresh
                        if msg_type.as_deref() == Some("error") {
                            let mut tabs_ref = tabs.borrow_mut();
                            let active = tabs_ref.active_tab_mut();
                            active.session_id = None;
                            let cols = active.grid.cols;
                            let rows = active.grid.rows;
                            drop(tabs_ref);

                            let create_msg = format!(
                                r#"{{"type":"create","cols":{},"rows":{}}}"#,
                                cols, rows
                            );
                            let state = ws_state.borrow();
                            if let Some(ref ws) = state.ws {
                                let _ = ws.send_with_str(&create_msg);
                            }
                            log::info!("Attach failed, creating new session");
                        }
                    }
                    return;
                }

                // Binary messages: first 16 bytes = session UUID, rest = PTY output
                if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&buffer);
                    let data = array.to_vec();
                    if data.len() > 16 {
                        let sid: [u8; 16] = data[..16].try_into().unwrap();
                        let pty_output = &data[16..];
                        tabs.borrow_mut().route_output(&sid, pty_output);
                    }
                }
            },
        );
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
    }

    // on_close / on_error -- schedule reconnect with exponential backoff
    {
        let ws_state_close = ws_state.clone();
        let tabs_close = tabs.clone();
        let url_close = url.clone();
        let on_close = Closure::<dyn FnMut()>::new(move || {
            log::info!("WebSocket closed, scheduling reconnect");
            schedule_reconnect(&ws_state_close, &tabs_close, &url_close);
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();
    }

    {
        let ws_state_err = ws_state.clone();
        let tabs_err = tabs.clone();
        let url_err = url.clone();
        let on_error = Closure::<dyn FnMut()>::new(move || {
            log::info!("WebSocket error, scheduling reconnect");
            schedule_reconnect(&ws_state_err, &tabs_err, &url_err);
        });
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();
    }

    ws_state.borrow_mut().ws = Some(ws);
}

fn schedule_reconnect(
    ws_state: &Rc<RefCell<WsState>>,
    tabs: &Rc<RefCell<TabManager>>,
    url: &Rc<String>,
) {
    let mut state = ws_state.borrow_mut();
    // Exponential backoff: 1s, 2s, 4s, 8s, ... max 30s
    state.backoff_ms = if state.backoff_ms == 0 {
        1000
    } else {
        (state.backoff_ms * 2).min(30_000)
    };
    let delay = state.backoff_ms;
    drop(state);

    let ws_state = ws_state.clone();
    let tabs = tabs.clone();
    let url = url.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        connect_ws(&ws_state, &tabs, &url);
    });
    web_sys::window()
        .unwrap()
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.as_ref().unchecked_ref(),
            delay as i32,
        )
        .unwrap();
    cb.forget();

    log::info!("Reconnecting in {delay}ms");
}

/// Send bytes over the WebSocket with session UUID prefix
fn ws_send_binary(ws_state: &RefCell<WsState>, session_id: &[u8; 16], payload: &[u8]) {
    let state = ws_state.borrow();
    let Some(ref ws) = state.ws else {
        return;
    };
    if ws.ready_state() != web_sys::WebSocket::OPEN {
        return;
    }

    let mut frame = session_id.to_vec();
    frame.extend_from_slice(payload);
    let array = js_sys::Uint8Array::from(&frame[..]);
    let _ = ws.send_with_array_buffer_view(&array);
}

/// Initialize a terminal inside the given container element
#[wasm_bindgen]
pub fn create_terminal(container_id: String, ws_url: String, font_size: f32) {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();

    wasm_bindgen_futures::spawn_local(async_main(container_id, ws_url, font_size));
}

async fn async_main(container_id: String, ws_url: String, font_size: f32) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let container: HtmlElement = document
        .get_element_by_id(&container_id)
        .unwrap_or_else(|| panic!("no element with id '{container_id}'"))
        .unchecked_into();
    container
        .style()
        .set_property("position", "relative")
        .unwrap();
    container
        .style()
        .set_property("overflow", "hidden")
        .unwrap();

    // Create tab bar first so canvas sits below it
    create_tab_bar(&container);

    let (canvas, canvas_id) = get_or_create_canvas(&container);
    let (ime_textarea, ime_overlay) = create_ime_elements(&container);
    let dpr = window.device_pixel_ratio() as f32;

    let width = canvas.width() as f32;
    let height = canvas.height() as f32;

    let sugarloaf_window = SugarloafWindow {
        handle: RawWindowHandle::Web(WebWindowHandle::new(canvas_id)),
        display: RawDisplayHandle::Web(WebDisplayHandle::new()),
        size: SugarloafWindowSize { width, height },
        scale: dpr,
    };

    let layout = RootStyle {
        font_size,
        line_height: 1.2,
        scale_factor: dpr,
    };

    let font_library = sugarloaf::font::FontLibrary::default();

    let mut sugarloaf = Sugarloaf::new_async(
        sugarloaf_window,
        SugarloafRenderer::default(),
        &font_library,
        layout,
    )
    .await
    .expect("Failed to create sugarloaf");

    let rt_id = sugarloaf.create_rich_text();

    // Calculate cell dimensions once (stable -- based on font size, not surface size)
    let dims = sugarloaf.get_rich_text_dimensions(&rt_id);
    let cell_width = dims.width * dpr;
    let cell_height = dims.height * dpr;

    let cols = if cell_width > 0.0 {
        (width / cell_width).max(1.0) as usize
    } else {
        80
    };
    let rows = if cell_height > 0.0 {
        (height / cell_height).max(1.0) as usize
    } else {
        24
    };

    log::info!("Terminal dimensions: {cols}x{rows} (cell: {cell_width}x{cell_height})");

    let tabs = Rc::new(RefCell::new(TabManager::new(cols, rows)));

    sugarloaf.set_background_color(Some(wgpu::Color {
        r: 0.05,
        g: 0.05,
        b: 0.1,
        a: 1.0,
    }));

    // WebSocket connection with auto-reconnect
    let ws_url = Rc::new(ws_url);
    let ws_state = Rc::new(RefCell::new(WsState {
        ws: None,
        backoff_ms: 0,
    }));
    connect_ws(&ws_state, &tabs, &ws_url);

    // Build the initial tab bar
    rebuild_tab_bar(&tabs, &ws_state);

    // IME composition state -- shared between keyboard and composition handlers
    let is_composing = Rc::new(RefCell::new(false));

    // Keyboard handler -- send input to WebSocket
    {
        let ws_state_key = ws_state.clone();
        let tabs_key = tabs.clone();
        let ws_state_paste = ws_state.clone();
        let tabs_paste = tabs.clone();
        let canvas_element: web_sys::EventTarget = canvas.clone().into();
        let textarea_target: web_sys::EventTarget = ime_textarea.clone().into();

        // Tab keyboard shortcuts
        let tabs_shortcut = tabs.clone();
        let ws_state_shortcut = ws_state.clone();

        let is_composing_ref = is_composing.clone();
        let on_keydown = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |event: web_sys::KeyboardEvent| {
                // Skip during IME composition
                if *is_composing_ref.borrow() {
                    return;
                }

                // Ctrl+T: create new tab
                if event.ctrl_key() && event.key() == "t" {
                    event.prevent_default();
                    let (cols, rows) = {
                        let tabs_ref = tabs_shortcut.borrow();
                        let active = tabs_ref.active_tab();
                        (active.grid.cols, active.grid.rows)
                    };
                    let new_idx = tabs_shortcut.borrow_mut().add_tab(cols, rows);
                    tabs_shortcut.borrow_mut().switch_to(new_idx);

                    // Send create message for the new tab
                    let create_msg =
                        format!(r#"{{"type":"create","cols":{},"rows":{}}}"#, cols, rows);
                    let state = ws_state_shortcut.borrow();
                    if let Some(ref ws) = state.ws {
                        if ws.ready_state() == web_sys::WebSocket::OPEN {
                            let _ = ws.send_with_str(&create_msg);
                        }
                    }
                    drop(state);

                    rebuild_tab_bar(&tabs_shortcut, &ws_state_shortcut);
                    return;
                }

                // Ctrl+W: close active tab
                if event.ctrl_key() && event.key() == "w" {
                    event.prevent_default();
                    let active_idx = tabs_shortcut.borrow().active;
                    let sid = tabs_shortcut.borrow_mut().close_tab(active_idx);
                    if let Some(sid) = sid {
                        let close_msg = format!(
                            r#"{{"type":"close","session_id":"{}"}}"#,
                            uuid::Uuid::from_bytes(sid)
                        );
                        let state = ws_state_shortcut.borrow();
                        if let Some(ref ws) = state.ws {
                            if ws.ready_state() == web_sys::WebSocket::OPEN {
                                let _ = ws.send_with_str(&close_msg);
                            }
                        }
                        rebuild_tab_bar(&tabs_shortcut, &ws_state_shortcut);
                    }
                    return;
                }

                // Let Ctrl+V through so the browser paste event fires
                if event.ctrl_key() && event.key() == "v" {
                    return;
                }
                event.prevent_default();

                // Clear any active text selection on keyboard input
                tabs_key
                    .borrow_mut()
                    .active_tab_mut()
                    .grid
                    .selection_clear();

                let bytes = key_event_to_bytes(&event);
                if bytes.is_empty() {
                    return;
                }

                let tabs_ref = tabs_key.borrow();
                let Some(sid) = tabs_ref.active_tab().session_id else {
                    return;
                };
                drop(tabs_ref);
                ws_send_binary(&ws_state_key, &sid, &bytes);
                tabs_key
                    .borrow_mut()
                    .active_tab_mut()
                    .grid
                    .scroll_to_bottom();
            },
        );
        textarea_target
            .add_event_listener_with_callback(
                "keydown",
                on_keydown.as_ref().unchecked_ref(),
            )
            .unwrap();
        on_keydown.forget();

        // Focus textarea on canvas click
        let textarea_for_focus = ime_textarea.clone();
        let on_click = Closure::<dyn FnMut()>::new(move || {
            textarea_for_focus.focus().unwrap();
        });
        canvas_element
            .add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())
            .unwrap();
        on_click.forget();

        // Paste handler -- send clipboard text as bracketed paste
        let on_paste = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::new(
            move |event: web_sys::ClipboardEvent| {
                event.prevent_default();

                let Some(data) = event.clipboard_data() else {
                    return;
                };
                let Ok(text) = data.get_data("text/plain") else {
                    return;
                };
                if text.is_empty() {
                    return;
                }

                // Bracketed paste: \x1b[200~ + text + \x1b[201~
                let mut payload = Vec::new();
                payload.extend_from_slice(b"\x1b[200~");
                payload.extend_from_slice(text.as_bytes());
                payload.extend_from_slice(b"\x1b[201~");

                let sid = {
                    let tabs_ref = tabs_paste.borrow();
                    tabs_ref.active_tab().session_id
                };
                let Some(sid) = sid else {
                    return;
                };
                ws_send_binary(&ws_state_paste, &sid, &payload);
            },
        );
        textarea_target
            .add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref())
            .unwrap();
        on_paste.forget();

        // Composition event handlers -- IME lifecycle
        // compositionstart -- position overlay at cursor and show it
        {
            let is_composing = is_composing.clone();
            let tabs = tabs.clone();
            let textarea = ime_textarea.clone();
            let overlay = ime_overlay.clone();
            let canvas_for_ime = canvas.clone();
            let cw = cell_width;
            let ch = cell_height;
            let on_compositionstart =
                Closure::<dyn FnMut(web_sys::CompositionEvent)>::new(
                    move |_event: web_sys::CompositionEvent| {
                        *is_composing.borrow_mut() = true;

                        let dpr = web_sys::window().unwrap().device_pixel_ratio();
                        let tabs_ref = tabs.borrow();
                        let active = tabs_ref.active_tab();
                        let cursor_col = active.grid.cursor_col;
                        let cursor_row = active.grid.cursor_row;
                        drop(tabs_ref);

                        let canvas_el: &web_sys::Element = canvas_for_ime.as_ref();
                        let rect = canvas_el.get_bounding_client_rect();
                        let css_x = rect.left() + cursor_col as f64 * (cw as f64 / dpr);
                        let css_y = rect.top() + cursor_row as f64 * (ch as f64 / dpr);

                        // Position the textarea at the cursor so the OS IME window
                        // appears near the insertion point
                        let ta_style = textarea.style();
                        ta_style
                            .set_property("left", &format!("{}px", css_x))
                            .unwrap();
                        ta_style
                            .set_property("top", &format!("{}px", css_y))
                            .unwrap();

                        // Position and show the overlay
                        let ov_style = overlay.style();
                        ov_style
                            .set_property("left", &format!("{}px", css_x))
                            .unwrap();
                        ov_style
                            .set_property("top", &format!("{}px", css_y))
                            .unwrap();
                        ov_style.set_property("display", "block").unwrap();
                    },
                );
            textarea_target
                .add_event_listener_with_callback(
                    "compositionstart",
                    on_compositionstart.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_compositionstart.forget();
        }

        // compositionupdate -- update overlay text with the preedit string
        {
            let overlay = ime_overlay.clone();
            let on_compositionupdate =
                Closure::<dyn FnMut(web_sys::CompositionEvent)>::new(
                    move |event: web_sys::CompositionEvent| {
                        if let Some(data) = event.data() {
                            overlay.set_text_content(Some(&data));
                        }
                    },
                );
            textarea_target
                .add_event_listener_with_callback(
                    "compositionupdate",
                    on_compositionupdate.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_compositionupdate.forget();
        }

        // compositionend -- commit text to PTY, hide overlay, clear textarea
        {
            let is_composing = is_composing.clone();
            let ws_state = ws_state.clone();
            let tabs = tabs.clone();
            let overlay = ime_overlay.clone();
            let textarea = ime_textarea.clone();
            let on_compositionend = Closure::<dyn FnMut(web_sys::CompositionEvent)>::new(
                move |event: web_sys::CompositionEvent| {
                    *is_composing.borrow_mut() = false;

                    // Hide and clear the overlay
                    overlay.style().set_property("display", "none").unwrap();
                    overlay.set_text_content(None);

                    // Send committed text to PTY as raw bytes
                    if let Some(data) = event.data() {
                        if !data.is_empty() {
                            let tabs_ref = tabs.borrow();
                            let Some(sid) = tabs_ref.active_tab().session_id else {
                                return;
                            };
                            drop(tabs_ref);
                            ws_send_binary(&ws_state, &sid, data.as_bytes());
                        }
                    }

                    // Clear the textarea so it's ready for the next composition
                    textarea.set_value("");
                },
            );
            textarea_target
                .add_event_listener_with_callback(
                    "compositionend",
                    on_compositionend.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_compositionend.forget();
        }

        // Mouse event handlers -- forward mouse input to the PTY when mouse mode is active
        let mouse_state = Rc::new(RefCell::new(MouseState {
            last_col: 0,
            last_row: 0,
            buttons_down: 0,
        }));

        // Text selection state
        let selecting = Rc::new(RefCell::new(false));

        // mousedown -- report press events to the PTY or start text selection
        {
            let tabs = tabs.clone();
            let ws_state = ws_state.clone();
            let mouse_state = mouse_state.clone();
            let selecting = selecting.clone();
            let cw = cell_width;
            let ch = cell_height;
            let on_mousedown = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
                move |event: web_sys::MouseEvent| {
                    let (col, row) =
                        pixel_to_cell(event.offset_x(), event.offset_y(), cw, ch);

                    let button = x11_button(event.button());
                    let mods = mouse_modifiers(&event);

                    {
                        let mut ms = mouse_state.borrow_mut();
                        ms.buttons_down |= 1 << button;
                        ms.last_col = col;
                        ms.last_row = row;
                    }

                    let mut tabs_ref = tabs.borrow_mut();
                    let active = tabs_ref.active_tab_mut();

                    // Start text selection when mouse mode is off
                    let mode = active.grid.mouse_mode();
                    if mode == MouseMode::None {
                        active.grid.selection_begin(col, row);
                        *selecting.borrow_mut() = true;
                        drop(tabs_ref);
                        return;
                    }

                    active.grid.mouse_report(button, mods, col, row, true);
                    let writes: Vec<u8> = active.grid.pending_writes.drain(..).collect();
                    let sid = active.session_id;
                    drop(tabs_ref);

                    if !writes.is_empty() {
                        if let Some(ref sid) = sid {
                            ws_send_binary(&ws_state, sid, &writes);
                        }
                        event.prevent_default();
                    }
                },
            );
            canvas_element
                .add_event_listener_with_callback(
                    "mousedown",
                    on_mousedown.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_mousedown.forget();
        }

        // mouseup -- report release events to the PTY or finish text selection
        {
            let tabs = tabs.clone();
            let ws_state = ws_state.clone();
            let mouse_state = mouse_state.clone();
            let selecting = selecting.clone();
            let cw = cell_width;
            let ch = cell_height;
            let on_mouseup = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
                move |event: web_sys::MouseEvent| {
                    let (col, row) =
                        pixel_to_cell(event.offset_x(), event.offset_y(), cw, ch);

                    let button = x11_button(event.button());
                    let mods = mouse_modifiers(&event);

                    mouse_state.borrow_mut().buttons_down &= !(1 << button);

                    // Finish text selection and copy to clipboard
                    if *selecting.borrow() {
                        *selecting.borrow_mut() = false;
                        let mut tabs_ref = tabs.borrow_mut();
                        let active = tabs_ref.active_tab_mut();
                        active.grid.selection_update(col, row);
                        let text = active.grid.selected_text();
                        drop(tabs_ref);

                        if !text.is_empty() {
                            let clipboard =
                                web_sys::window().unwrap().navigator().clipboard();
                            let _ = clipboard.write_text(&text);
                        }
                        return;
                    }

                    let mut tabs_ref = tabs.borrow_mut();
                    let active = tabs_ref.active_tab_mut();
                    active.grid.mouse_report(button, mods, col, row, false);
                    let writes: Vec<u8> = active.grid.pending_writes.drain(..).collect();
                    let sid = active.session_id;
                    drop(tabs_ref);

                    if !writes.is_empty() {
                        if let Some(ref sid) = sid {
                            ws_send_binary(&ws_state, sid, &writes);
                        }
                    }
                },
            );
            canvas_element
                .add_event_listener_with_callback(
                    "mouseup",
                    on_mouseup.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_mouseup.forget();
        }

        // mousemove -- report motion events (drag or all-motion depending on mode)
        {
            let tabs = tabs.clone();
            let ws_state = ws_state.clone();
            let mouse_state = mouse_state.clone();
            let selecting = selecting.clone();
            let cw = cell_width;
            let ch = cell_height;
            let on_mousemove = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
                move |event: web_sys::MouseEvent| {
                    let (col, row) =
                        pixel_to_cell(event.offset_x(), event.offset_y(), cw, ch);

                    // Update text selection during drag
                    if *selecting.borrow() {
                        let mut tabs_ref = tabs.borrow_mut();
                        let active = tabs_ref.active_tab_mut();
                        active.grid.selection_update(col, row);
                        return;
                    }

                    let mut ms = mouse_state.borrow_mut();

                    // Skip if cell position hasn't changed
                    if col == ms.last_col && row == ms.last_row {
                        return;
                    }
                    ms.last_col = col;
                    ms.last_row = row;
                    let buttons_down = ms.buttons_down;
                    drop(ms);

                    let mut tabs_ref = tabs.borrow_mut();
                    let active = tabs_ref.active_tab_mut();
                    let mode = active.grid.mouse_mode();

                    // DragMotion only reports when a button is held; AllMotion always reports
                    let should_report = match mode {
                        MouseMode::AllMotion => true,
                        MouseMode::DragMotion => buttons_down != 0,
                        _ => false,
                    };
                    if !should_report {
                        return;
                    }

                    // Motion button code: 32 + held button (or 35 if no buttons held)
                    let button = if buttons_down != 0 {
                        32 + (buttons_down.trailing_zeros() as u8)
                    } else {
                        35
                    };
                    let mods = mouse_modifiers(&event);

                    active.grid.mouse_report(button, mods, col, row, true);
                    let writes: Vec<u8> = active.grid.pending_writes.drain(..).collect();
                    let sid = active.session_id;
                    drop(tabs_ref);

                    if !writes.is_empty() {
                        if let Some(ref sid) = sid {
                            ws_send_binary(&ws_state, sid, &writes);
                        }
                    }
                },
            );
            canvas_element
                .add_event_listener_with_callback(
                    "mousemove",
                    on_mousemove.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_mousemove.forget();
        }

        // wheel -- scrollback when mouse mode is off, otherwise report to PTY
        {
            let tabs = tabs.clone();
            let ws_state = ws_state.clone();
            let cw = cell_width;
            let ch = cell_height;
            let on_wheel = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(
                move |event: web_sys::WheelEvent| {
                    let mouse_event: &web_sys::MouseEvent = event.as_ref();
                    let (col, row) = pixel_to_cell(
                        mouse_event.offset_x(),
                        mouse_event.offset_y(),
                        cw,
                        ch,
                    );

                    // When mouse mode is off, scroll the viewport instead
                    let mode = tabs.borrow().active_tab().grid.mouse_mode();
                    if mode == MouseMode::None {
                        let lines = if event.delta_y() < 0.0 { 3 } else { -3 };
                        tabs.borrow_mut()
                            .active_tab_mut()
                            .grid
                            .scroll_display(lines);
                        event.prevent_default();
                        return;
                    }

                    let button: u8 = if event.delta_y() < 0.0 { 64 } else { 65 };
                    let mods = mouse_modifiers(mouse_event);

                    let mut tabs_ref = tabs.borrow_mut();
                    let active = tabs_ref.active_tab_mut();
                    active.grid.mouse_report(button, mods, col, row, true);
                    let writes: Vec<u8> = active.grid.pending_writes.drain(..).collect();
                    let sid = active.session_id;
                    drop(tabs_ref);

                    if !writes.is_empty() {
                        if let Some(ref sid) = sid {
                            ws_send_binary(&ws_state, sid, &writes);
                            event.prevent_default();
                        }
                    }
                },
            );
            canvas_element
                .add_event_listener_with_callback(
                    "wheel",
                    on_wheel.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_wheel.forget();
        }

        // contextmenu -- suppress right-click menu on the canvas
        {
            let on_contextmenu = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(
                move |event: web_sys::MouseEvent| {
                    event.prevent_default();
                },
            );
            canvas_element
                .add_event_listener_with_callback(
                    "contextmenu",
                    on_contextmenu.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_contextmenu.forget();
        }

        // Clear stale textarea content on non-composition input
        {
            let is_composing = is_composing.clone();
            let textarea = ime_textarea.clone();
            let on_input = Closure::<dyn FnMut(web_sys::InputEvent)>::new(
                move |_event: web_sys::InputEvent| {
                    if !*is_composing.borrow() {
                        textarea.set_value("");
                    }
                },
            );
            textarea_target
                .add_event_listener_with_callback(
                    "input",
                    on_input.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_input.forget();
        }

        // Re-focus textarea when the window regains focus
        {
            let textarea = ime_textarea.clone();
            let on_window_focus = Closure::<dyn FnMut()>::new(move || {
                let _ = textarea.focus();
            });
            web_sys::window()
                .unwrap()
                .add_event_listener_with_callback(
                    "focus",
                    on_window_focus.as_ref().unchecked_ref(),
                )
                .unwrap();
            on_window_focus.forget();
        }

        // Auto-focus textarea for keyboard/IME input
        ime_textarea.focus().unwrap();
    }

    let sugarloaf = Rc::new(RefCell::new(sugarloaf));

    // ResizeObserver -- debounced recalculation of terminal dimensions
    {
        let sugarloaf = sugarloaf.clone();
        let tabs = tabs.clone();
        let ws_state = ws_state.clone();
        let canvas_observe = canvas.clone();
        let pending_timer: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));

        let on_resize = Closure::<dyn FnMut(js_sys::Array)>::new(
            move |_entries: js_sys::Array| {
                let window = web_sys::window().unwrap();

                // Cancel any pending debounce timer
                if let Some(timer_id) = pending_timer.borrow_mut().take() {
                    window.clear_timeout_with_handle(timer_id);
                }

                // Schedule the actual resize after 50ms of inactivity
                let sugarloaf = sugarloaf.clone();
                let tabs = tabs.clone();
                let ws_state = ws_state.clone();
                let canvas_observe = canvas_observe.clone();
                let pending_timer_inner = pending_timer.clone();

                let cb = Closure::<dyn FnMut()>::once(move || {
                    *pending_timer_inner.borrow_mut() = None;

                    let window = web_sys::window().unwrap();
                    let dpr = window.device_pixel_ratio();

                    let css_width = canvas_observe.client_width() as f64;
                    let css_height = canvas_observe.client_height() as f64;
                    let px_width = (css_width * dpr) as u32;
                    let px_height = (css_height * dpr) as u32;

                    if px_width == 0 || px_height == 0 {
                        return;
                    }

                    canvas_observe.set_width(px_width);
                    canvas_observe.set_height(px_height);

                    let mut sugarloaf = sugarloaf.borrow_mut();
                    sugarloaf.resize(px_width, px_height);
                    drop(sugarloaf);

                    let new_cols = if cell_width > 0.0 {
                        (px_width as f32 / cell_width).max(1.0) as usize
                    } else {
                        80
                    };
                    let new_rows = if cell_height > 0.0 {
                        (px_height as f32 / cell_height).max(1.0) as usize
                    } else {
                        24
                    };

                    // Resize ALL tabs' grids and send resize messages for each active session
                    let mut tabs_ref = tabs.borrow_mut();
                    let state = ws_state.borrow();
                    for tab in &mut tabs_ref.tabs {
                        if new_cols != tab.grid.cols || new_rows != tab.grid.rows {
                            tab.grid.resize(new_cols, new_rows);

                            if let Some(sid) = tab.session_id.as_ref() {
                                let resize_msg = format!(
                                    r#"{{"type":"resize","session_id":"{}","cols":{},"rows":{}}}"#,
                                    uuid::Uuid::from_bytes(*sid),
                                    new_cols,
                                    new_rows
                                );
                                if let Some(ref ws) = state.ws {
                                    let _ = ws.send_with_str(&resize_msg);
                                }
                            }
                        }
                    }
                });

                let timer_id = window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.as_ref().unchecked_ref(),
                        50,
                    )
                    .unwrap();
                cb.forget();
                *pending_timer.borrow_mut() = Some(timer_id);
            },
        );

        let canvas_for_observe = canvas.clone();
        let observer =
            web_sys::ResizeObserver::new(on_resize.as_ref().unchecked_ref()).unwrap();
        observer.observe(&canvas_for_observe);
        on_resize.forget();
        std::mem::forget(observer);
    }

    // Render loop
    render_loop(sugarloaf, tabs, rt_id);
}

fn render_loop(
    sugarloaf: Rc<RefCell<Sugarloaf<'static>>>,
    tabs: Rc<RefCell<TabManager>>,
    rt_id: usize,
) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        {
            let mut tabs_ref = tabs.borrow_mut();
            let active = tabs_ref.active_tab_mut();
            if active.grid.dirty {
                let mut sugarloaf = sugarloaf.borrow_mut();
                render_grid(&mut sugarloaf, &active.grid, rt_id);
                sugarloaf.set_objects(vec![Object::RichText(RichText {
                    id: rt_id,
                    position: [0.0, 0.0],
                    lines: None,
                })]);
                sugarloaf.render();
                active.grid.dirty = false;
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}

/// Convert a browser keyboard event to terminal input bytes
fn key_event_to_bytes(event: &web_sys::KeyboardEvent) -> Vec<u8> {
    let key = event.key();
    let ctrl = event.ctrl_key();
    let alt = event.alt_key();

    // Handle special keys
    match key.as_str() {
        "Enter" => return b"\r".to_vec(),
        "Backspace" => return vec![0x7f],
        "Tab" => return b"\t".to_vec(),
        "Escape" => return vec![0x1b],
        "ArrowUp" => return b"\x1b[A".to_vec(),
        "ArrowDown" => return b"\x1b[B".to_vec(),
        "ArrowRight" => return b"\x1b[C".to_vec(),
        "ArrowLeft" => return b"\x1b[D".to_vec(),
        "Home" => return b"\x1b[H".to_vec(),
        "End" => return b"\x1b[F".to_vec(),
        "PageUp" => return b"\x1b[5~".to_vec(),
        "PageDown" => return b"\x1b[6~".to_vec(),
        "Insert" => return b"\x1b[2~".to_vec(),
        "Delete" => return b"\x1b[3~".to_vec(),
        "F1" => return b"\x1bOP".to_vec(),
        "F2" => return b"\x1bOQ".to_vec(),
        "F3" => return b"\x1bOR".to_vec(),
        "F4" => return b"\x1bOS".to_vec(),
        "F5" => return b"\x1b[15~".to_vec(),
        "F6" => return b"\x1b[17~".to_vec(),
        "F7" => return b"\x1b[18~".to_vec(),
        "F8" => return b"\x1b[19~".to_vec(),
        "F9" => return b"\x1b[20~".to_vec(),
        "F10" => return b"\x1b[21~".to_vec(),
        "F11" => return b"\x1b[23~".to_vec(),
        "F12" => return b"\x1b[24~".to_vec(),
        _ => {}
    }

    // Ctrl+key combinations (skip Ctrl+V -- let browser paste event handle it)
    if ctrl && key.len() == 1 {
        let ch = key.chars().next().unwrap();
        if ch.to_ascii_lowercase() == 'v' {
            return vec![];
        }
        if ch.is_ascii_alphabetic() {
            let ctrl_byte = (ch.to_ascii_lowercase() as u8) - b'a' + 1;
            return vec![ctrl_byte];
        }
    }

    // Alt+key: send ESC prefix
    if alt && key.len() == 1 {
        let mut bytes = vec![0x1b];
        bytes.extend_from_slice(key.as_bytes());
        return bytes;
    }

    // Regular character input
    if key.len() == 1 || key.chars().count() == 1 {
        return key.as_bytes().to_vec();
    }

    vec![]
}
