---
"@omnidotdev/terminal": minor
---

Initial public release of Omni Terminal — a GPU-accelerated terminal emulator built to run everywhere.

### Multi-platform support

- **Desktop** — native builds for macOS (universal binary), Linux (X11 and Wayland), and Windows (MSI installer)
- **Web** — WebAssembly frontend with WebGPU rendering to HTML canvas, powered by an Axum-based WebSocket PTY server
- **Android** — native app with Vulkan rendering, Arch Linux rootfs via proot, and bundled busybox bootstrap environment

### Terminal features

- Multi-session tab management with keyboard shortcuts (Ctrl+T / Ctrl+W) and mouse interaction
- Text selection with copy-on-select (click-drag on web, long-press on Android)
- Contextual Ctrl+C — copies selected text or sends SIGINT when nothing is selected
- Scrollback buffer with touch-to-scroll (Android) and mouse wheel support (web/desktop)
- Bracketed paste, mouse reporting (SGR modes 1000/1002/1003/1006), and IME input support
- Runtime opacity controls via keybindings
- Nerd Font support with per-character font fallback
- Dynamic background color updates via OSC 11

### Network and security

- Auto-generated self-signed TLS certificates for secure WebSocket connections
- Per-session routing with UUID-based output forwarding
- Session persistence across WebSocket reconnects with exponential backoff
- All local IPs included in certificate SANs for flexible network access

### Build and packaging

- Homebrew tap, AUR (binary and source), Flatpak, and WinGet packaging
- macOS universal DMG (x86_64 + aarch64 via lipo)
- Linux .deb packages for X11 and Wayland, plus tarball distribution
- Optimized release profile — LTO, symbol stripping, panic abort, single codegen unit
