// clipboard.rs was retired originally from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/clipboard.rs
// which is licensed under Apache 2.0 license.

use raw_window_handle::RawDisplayHandle;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardType {
    Clipboard,
    Selection,
}

use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use copypasta::x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext};
#[cfg(any(feature = "x11", target_os = "macos", windows))]
use copypasta::ClipboardContext;
use copypasta::ClipboardProvider;

/// Command-line Wayland clipboard provider using `wl-paste`/`wl-copy`.
///
/// Replaces copypasta's smithay-clipboard backend, which uses `.unwrap()` on
/// mutex locks and FFI calls internally. With `panic = "abort"`, those panics
/// kill the process instantly (the Ctrl+V crash). Subprocess calls cannot panic.
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
struct WaylandCmdClipboard {
    primary: bool,
}

#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
impl WaylandCmdClipboard {
    fn new(primary: bool) -> Self {
        Self { primary }
    }
}

#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
impl ClipboardProvider for WaylandCmdClipboard {
    fn get_contents(
        &mut self,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = std::process::Command::new("wl-paste");
        cmd.arg("--no-newline");
        if self.primary {
            cmd.arg("--primary");
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let output = cmd.output()?;
        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?)
        } else {
            Err("wl-paste failed".into())
        }
    }

    fn set_contents(
        &mut self,
        data: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = std::process::Command::new("wl-copy");
        if self.primary {
            cmd.arg("--primary");
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            std::io::Write::write_all(&mut stdin, data.as_bytes())?;
            // Drop stdin to close pipe so wl-copy sees EOF
        }
        child.wait()?;
        Ok(())
    }
}

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
}

impl Clipboard {
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn new(display: RawDisplayHandle) -> Self {
        match display {
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(_) => Self {
                clipboard: Box::new(WaylandCmdClipboard::new(false)),
                selection: Some(Box::new(WaylandCmdClipboard::new(true))),
            },
            _ => Self::default(),
        }
    }

    /// Used for tests and to handle missing clipboard provider when built without the `x11`
    /// feature.
    pub fn new_nop() -> Self {
        Self {
            clipboard: Box::new(NopClipboardContext::new().unwrap()),
            selection: None,
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", windows))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: None,
        };

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: Some(Box::new(
                X11ClipboardContext::<X11SelectionClipboard>::new().unwrap(),
            )),
        };

        #[cfg(not(any(feature = "x11", target_os = "macos", windows)))]
        return Self::new_nop();
    }
}

impl Clipboard {
    pub fn set(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let result = if ty == ClipboardType::Selection {
            if let Some(provider) = &mut self.selection {
                provider.set_contents(text.into())
            } else {
                return;
            }
        } else {
            self.clipboard.set_contents(text.into())
        };

        if let Err(err) = result {
            warn!("Unable to store text in clipboard: {}", err);
        }
    }

    pub fn get(&mut self, ty: ClipboardType) -> String {
        let result = if ty == ClipboardType::Selection {
            if let Some(provider) = &mut self.selection {
                provider.get_contents()
            } else {
                self.clipboard.get_contents()
            }
        } else {
            self.clipboard.get_contents()
        };

        match result {
            Err(err) => {
                warn!("Unable to load text from clipboard: {}", err);
                String::new()
            }
            Ok(text) => text,
        }
    }
}
