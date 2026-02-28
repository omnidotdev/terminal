# @omnidotdev/terminal

## 0.1.0

### Minor Changes

- [`a304ec1`](https://github.com/omnidotdev/terminal/commit/a304ec1d987b6748ae4c99dba8a6baac88dab7a1) Thanks [@coopbri](https://github.com/coopbri)! - Initial public release of Omni Terminal — a GPU-accelerated terminal emulator built to run everywhere.

  ### Multi-platform support

  - **Desktop** — native builds for macOS (universal binary), Linux (X11 and Wayland), and Windows (MSI installer)
  - **Web** — WebAssembly frontend with WebGPU rendering to HTML canvas, powered by an Axum-based WebSocket PTY server
  - **Android** — native app with Vulkan rendering, Arch Linux rootfs via proot with /sdcard binding, bundled busybox and terminfo bootstrap, foreground service for background persistence, and settings UI with theme selection and persistent preferences
  - **NPM** — distributed as `@omnidotdev/terminal` for embedding the web terminal

  ### Terminal features

  - Multi-session tab management with keyboard shortcuts (Ctrl+T / Ctrl+W) and mouse interaction
  - Text selection with copy-on-select (click-drag on web, long-press on Android)
  - Contextual Ctrl+C — copies selected text or sends SIGINT when nothing is selected
  - Scrollback buffer with touch-to-scroll and scroll position indicator (Android) and mouse wheel support (web/desktop)
  - Auto-scroll to bottom on user input
  - Bracketed paste, mouse reporting (SGR modes 1000/1002/1003/1006), and IME input support
  - Runtime opacity controls via keybindings
  - Nerd Font support with per-character font fallback
  - Dynamic background color updates via OSC 11
  - Dismissable config error banner instead of blocking error screen

  ### Network and security

  - Auto-generated self-signed TLS certificates for secure WebSocket connections
  - Per-session routing with UUID-based output forwarding
  - Session persistence across WebSocket reconnects with exponential backoff
  - All local IPs included in certificate SANs for flexible network access

  ### Build and packaging

  - Homebrew tap and AUR (binary and source) packaging
  - macOS universal DMG (x86_64 + aarch64 via lipo)
  - Linux .deb packages for X11 and Wayland, plus tarball distribution
  - Optimized release profile — LTO, symbol stripping, panic abort, single codegen unit
