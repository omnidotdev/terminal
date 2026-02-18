pub mod sync;

use crate::ansi::graphics::UpdateQueues;
use crate::clipboard::ClipboardType;
use crate::config::colors::ColorRgb;
use crate::crosswords::grid::Scroll;
use crate::crosswords::pos::{Direction, Pos};
use crate::crosswords::search::{Match, RegexSearch};
use crate::crosswords::LineDamage;
use crate::error::TerminalError;
use terminal_window::event::Event as TerminalWindowEvent;
use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use teletypewriter::WinsizeBuilder;

use terminal_window::event_loop::EventLoopProxy;

pub type WindowId = terminal_window::window::WindowId;

#[derive(Debug, Clone)]
pub enum TerminalEventType {
    Terminal(TerminalEvent),
    Frame,
    // Message(Message),
}

#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    #[allow(dead_code)]
    Shutdown,

    Resize(WinsizeBuilder),
}

#[derive(Debug, Eq, PartialEq)]
pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
}

/// Terminal damage information for efficient rendering
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalDamage {
    /// The entire terminal needs to be redrawn
    Full,
    /// Only specific lines need to be redrawn
    Partial(BTreeSet<LineDamage>),
    /// Only the cursor position has changed
    CursorOnly,
}

#[derive(Clone)]
pub enum TerminalEvent {
    PrepareRender(u64),
    PrepareRenderOnRoute(u64, usize),
    PrepareUpdateConfig,
    /// New terminal content available.
    Render,
    /// New terminal content available per route.
    RenderRoute(usize),
    /// Wake up and check for terminal updates.
    Wakeup(usize),
    /// Graphics update available from terminal.
    UpdateGraphics {
        route_id: usize,
        queues: UpdateQueues,
    },
    Paste,
    Copy(String),
    UpdateFontSize(u8),
    Scroll(Scroll),
    ToggleFullScreen,
    Minimize(bool),
    Hide,
    HideOtherApplications,
    UpdateConfig,
    CreateWindow,
    CloseWindow,
    CreateNativeTab(Option<String>),
    CreateConfigEditor,
    SelectNativeTabByIndex(usize),
    SelectNativeTabLast,
    SelectNativeTabNext,
    SelectNativeTabPrev,

    ReportToAssistant(TerminalError),

    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Window title change.
    TitleWithSubtitle(String, String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// The attached function is a formatter which will correctly transform the clipboard content
    /// into the expected escape sequence format.
    ClipboardLoad(
        ClipboardType,
        Arc<dyn Fn(&str) -> String + Sync + Send + 'static>,
    ),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// The attached function is a formatter which will correctly transform the RGB color into the
    /// expected escape sequence format.
    ColorRequest(
        usize,
        Arc<dyn Fn(ColorRgb) -> String + Sync + Send + 'static>,
    ),

    /// Write some text to the PTY.
    PtyWrite(String),

    /// Request to write the text area size.
    TextAreaSizeRequest(Arc<dyn Fn(WinsizeBuilder) -> String + Sync + Send + 'static>),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    CursorBlinkingChangeOnRoute(usize),

    /// Terminal bell ring.
    Bell,

    /// Shutdown request.
    Exit,

    /// Quit request.
    Quit,

    /// Leave current terminal.
    CloseTerminal(usize),

    BlinkCursor(u64, usize),

    /// Update window titles.
    UpdateTitles,

    /// Update terminal screen colors.
    ///
    /// The first usize is the route_id, the second is the color index to change.
    /// Color index: 0 for foreground, 1 for background, 2 for cursor color.
    ColorChange(usize, usize, Option<ColorRgb>),

    // No operation
    Noop,
}

impl Debug for TerminalEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalEvent::ClipboardStore(ty, text) => {
                write!(f, "ClipboardStore({ty:?}, {text})")
            }
            TerminalEvent::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({ty:?})"),
            TerminalEvent::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
            TerminalEvent::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
            TerminalEvent::PtyWrite(text) => write!(f, "PtyWrite({text})"),
            TerminalEvent::Title(title) => write!(f, "Title({title})"),
            TerminalEvent::TitleWithSubtitle(title, subtitle) => {
                write!(f, "TitleWithSubtitle({title}, {subtitle})")
            }
            TerminalEvent::Minimize(cond) => write!(f, "Minimize({cond})"),
            TerminalEvent::Hide => write!(f, "Hide)"),
            TerminalEvent::HideOtherApplications => write!(f, "HideOtherApplications)"),
            TerminalEvent::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            TerminalEvent::CursorBlinkingChangeOnRoute(route_id) => {
                write!(f, "CursorBlinkingChangeOnRoute {route_id}")
            }
            TerminalEvent::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            TerminalEvent::ResetTitle => write!(f, "ResetTitle"),
            TerminalEvent::PrepareUpdateConfig => write!(f, "PrepareUpdateConfig"),
            TerminalEvent::PrepareRender(millis) => write!(f, "PrepareRender({millis})"),
            TerminalEvent::PrepareRenderOnRoute(millis, route) => {
                write!(f, "PrepareRender({millis} on route {route})")
            }
            TerminalEvent::Render => write!(f, "Render"),
            TerminalEvent::RenderRoute(route) => write!(f, "Render route {route}"),
            TerminalEvent::Wakeup(route) => {
                write!(f, "Wakeup route {route}")
            }
            TerminalEvent::Scroll(scroll) => write!(f, "Scroll {scroll:?}"),
            TerminalEvent::Bell => write!(f, "Bell"),
            TerminalEvent::Exit => write!(f, "Exit"),
            TerminalEvent::Quit => write!(f, "Quit"),
            TerminalEvent::CloseTerminal(route) => write!(f, "CloseTerminal {route}"),
            TerminalEvent::CreateWindow => write!(f, "CreateWindow"),
            TerminalEvent::CloseWindow => write!(f, "CloseWindow"),
            TerminalEvent::CreateNativeTab(_) => write!(f, "CreateNativeTab"),
            TerminalEvent::SelectNativeTabByIndex(tab_index) => {
                write!(f, "SelectNativeTabByIndex({tab_index})")
            }
            TerminalEvent::SelectNativeTabLast => write!(f, "SelectNativeTabLast"),
            TerminalEvent::SelectNativeTabNext => write!(f, "SelectNativeTabNext"),
            TerminalEvent::SelectNativeTabPrev => write!(f, "SelectNativeTabPrev"),
            TerminalEvent::CreateConfigEditor => write!(f, "CreateConfigEditor"),
            TerminalEvent::UpdateConfig => write!(f, "ReloadConfiguration"),
            TerminalEvent::ReportToAssistant(error_report) => {
                write!(f, "ReportToAssistant({})", error_report.report)
            }
            TerminalEvent::ToggleFullScreen => write!(f, "FullScreen"),
            TerminalEvent::BlinkCursor(timeout, route_id) => {
                write!(f, "BlinkCursor {timeout} {route_id}")
            }
            TerminalEvent::UpdateTitles => write!(f, "UpdateTitles"),
            TerminalEvent::Noop => write!(f, "Noop"),
            TerminalEvent::Copy(_) => write!(f, "Copy"),
            TerminalEvent::Paste => write!(f, "Paste"),
            TerminalEvent::UpdateFontSize(action) => write!(f, "UpdateFontSize({action:?})"),
            TerminalEvent::UpdateGraphics { route_id, .. } => {
                write!(f, "UpdateGraphics({route_id})")
            }
            TerminalEvent::ColorChange(route_id, color, rgb) => {
                write!(f, "ColorChange({route_id}, {color:?}, {rgb:?})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventPayload {
    /// Event payload.
    pub payload: TerminalEventType,
    pub window_id: WindowId,
}

impl EventPayload {
    pub fn new(payload: TerminalEventType, window_id: WindowId) -> Self {
        Self { payload, window_id }
    }
}

impl From<EventPayload> for TerminalWindowEvent<EventPayload> {
    fn from(event: EventPayload) -> Self {
        TerminalWindowEvent::UserEvent(event)
    }
}

pub trait OnResize {
    fn on_resize(&mut self, window_size: WinsizeBuilder);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn event(&self) -> (Option<TerminalEvent>, bool);

    fn send_event(&self, _event: TerminalEvent, _id: WindowId) {}

    fn send_event_with_high_priority(&self, _event: TerminalEvent, _id: WindowId) {}

    fn send_redraw(&self, _id: WindowId) {}

    fn send_global_event(&self, _event: TerminalEvent) {}
}

#[derive(Clone)]
pub struct VoidListener;

impl From<TerminalEvent> for TerminalEventType {
    fn from(terminal_event: TerminalEvent) -> Self {
        Self::Terminal(terminal_event)
    }
}

impl EventListener for VoidListener {
    fn event(&self) -> (std::option::Option<TerminalEvent>, bool) {
        (None, false)
    }
}

#[derive(Debug, Clone)]
pub struct EventProxy {
    proxy: EventLoopProxy<EventPayload>,
}

impl EventProxy {
    pub fn new(proxy: EventLoopProxy<EventPayload>) -> Self {
        Self { proxy }
    }

    pub fn send_event(&self, event: TerminalEventType, id: WindowId) {
        let _ = self.proxy.send_event(EventPayload::new(event, id));
    }
}

impl EventListener for EventProxy {
    fn event(&self) -> (std::option::Option<TerminalEvent>, bool) {
        (None, false)
    }

    fn send_event(&self, event: TerminalEvent, id: WindowId) {
        let _ = self.proxy.send_event(EventPayload::new(event.into(), id));
    }
}

/// Regex search state.
pub struct SearchState {
    /// Search direction.
    pub direction: Direction,

    /// Current position in the search history.
    pub history_index: Option<usize>,

    /// Change in display offset since the beginning of the search.
    pub display_offset_delta: i32,

    /// Search origin in viewport coordinates relative to original display offset.
    pub origin: Pos,

    /// Focused match during active search.
    pub focused_match: Option<Match>,

    /// Search regex and history.
    ///
    /// During an active search, the first element is the user's current input.
    ///
    /// While going through history, the [`SearchState::history_index`] will point to the element
    /// in history which is currently being previewed.
    pub history: VecDeque<String>,

    /// Compiled search automatons.
    pub dfas: Option<RegexSearch>,
}

impl SearchState {
    /// Search regex text if a search is active.
    pub fn regex(&self) -> Option<&String> {
        self.history_index.and_then(|index| self.history.get(index))
    }

    /// Direction of the search from the search origin.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Focused match during vi-less search.
    pub fn focused_match(&self) -> Option<&Match> {
        self.focused_match.as_ref()
    }

    /// Clear the focused match.
    pub fn clear_focused_match(&mut self) {
        self.focused_match = None;
    }

    /// Active search dfas.
    pub fn dfas_mut(&mut self) -> Option<&mut RegexSearch> {
        self.dfas.as_mut()
    }

    /// Active search dfas.
    pub fn dfas(&self) -> Option<&RegexSearch> {
        self.dfas.as_ref()
    }

    /// Search regex text if a search is active.
    pub fn regex_mut(&mut self) -> Option<&mut String> {
        self.history_index
            .and_then(move |index| self.history.get_mut(index))
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            direction: Direction::Right,
            display_offset_delta: Default::default(),
            focused_match: Default::default(),
            history_index: Default::default(),
            history: Default::default(),
            origin: Default::default(),
            dfas: Default::default(),
        }
    }
}
