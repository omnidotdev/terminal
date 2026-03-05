# Omni Terminal

## 0.2.4

### Patch Changes

- [`3c389ee`](https://github.com/omnidotdev/terminal/commit/3c389ee693e8576e79627262280445203b0391f2) Thanks [@coopbri](https://github.com/coopbri)! - fix(renderer): uniform opacity and background color for TUI programs

  - Add dedicated replace-blend GPU pipeline for background rects to prevent alpha accumulation when drawing semi-transparent backgrounds over a semi-transparent clear color
  - Track background vertices separately in the compositor so cell backgrounds render with correct opacity
  - Force full damage on all contexts when opacity changes via keybinding so all cells re-render
  - Preserve current opacity when changing background color via OSC 11
  - Process background state changes before cell rendering so cells use the updated default background reference
  - Treat cells matching the original theme background as "default" so TUI programs that set explicit backgrounds still track OSC 11 background changes

## 0.2.3

### Patch Changes

- [`9ff8651`](https://github.com/omnidotdev/terminal/commit/9ff8651a580c45bdab1414292d541d5a4482df72) Thanks [@coopbri](https://github.com/coopbri)! - fix(wasm): mobile browser support for remote terminal access

  - Fix terminal text wrapping at wrong column on high-DPI devices by removing DPR double-counting in cell dimension calculation
  - Fix Android virtual keyboard not appearing by adding touchend focus handler and forwarding textarea input events to the PTY
  - Fix iOS rendering by gracefully handling sugarloaf font warnings instead of discarding the working GPU instance
  - Add visible panic overlay for mobile debugging (no console access on mobile browsers)
  - Make hidden textarea full-size with proper styling to ensure mobile browsers treat it as focusable

- [`0bece69`](https://github.com/omnidotdev/terminal/commit/0bece69c6c61958f1c03ad0015f2dc244dab8faf) Thanks [@coopbri](https://github.com/coopbri)! - fix(wasm): process exit UX and iOS Safari rendering

  - Show "[Process exited. Press Enter to restart.]" when shell exits instead of leaving a frozen terminal
  - Restart session on Enter after process exit (desktop keydown + mobile input)
  - Server sends `exited` WebSocket message when PTY reader detects shell exit (EIO/EOF)
  - Fix iOS Safari rendering by forcing WebGL backend (Safari's WebGPU has device-loss issues during glyph rendering)
  - Enable wgpu `webgl` feature for WebGL2 fallback support on WASM targets
  - Detect iOS/iPadOS via user agent and maxTouchPoints for backend selection
  - Fix FullRender cache path advancing by one cell width instead of full run width

## 0.2.2

### Patch Changes

- [`508fd31`](https://github.com/omnidotdev/terminal/commit/508fd319a5025519aaef2298fb347a0c0aa50447) Thanks [@coopbri](https://github.com/coopbri)! - Fix WASM frontend 404 on `omni-terminal serve` by building WASM artifacts in CI release workflow before compiling release binaries

## 0.2.1

### Patch Changes

- [`6a13bab`](https://github.com/omnidotdev/terminal/commit/6a13bab2c7ce5ef96e74e97dc01dd40e94cf95c1) Thanks [@coopbri](https://github.com/coopbri)! - Fix AUR PKGBUILD to build WASM frontend before main binary, resolving 404 on `omni-terminal serve`

## 0.2.0

### Minor Changes

- [`963a7a3`](https://github.com/omnidotdev/terminal/commit/963a7a36f39dbdf47b923debf82e65c00eaf6534) Thanks [@coopbri](https://github.com/coopbri)! - Add `omni-terminal serve` subcommand that starts a WebSocket PTY server with the WASM frontend embedded in the binary. Defaults to HTTPS with auto-generated self-signed certificate on 127.0.0.1:3000. Behind a default-on `serve` Cargo feature flag. Removes the standalone `web-server` crate.

## 0.1.3

### Patch Changes

- [`1b32bb7`](https://github.com/omnidotdev/terminal/commit/1b32bb7fe950d55727fb68372e1620c2f15d1605) Thanks [@coopbri](https://github.com/coopbri)! - Switch release profile from `panic = "abort"` to `panic = "unwind"`, fixing Ctrl+V segfault (stack smashing) on Wayland

## 0.1.2

### Patch Changes

- [`41221b7`](https://github.com/omnidotdev/terminal/commit/41221b7487a7e673c8a6016338410cbc9d7d509d) Thanks [@coopbri](https://github.com/coopbri)! - Replace smithay-clipboard with wl-paste/wl-copy subprocess calls, fixing Ctrl+V crash on Wayland caused by internal `.unwrap()` panics under `panic = "abort"`

## 0.1.1

### Patch Changes

- [`c7afbd4`](https://github.com/omnidotdev/terminal/commit/c7afbd4cb698dad8561f863b8eafbf0409badc93) Thanks [@coopbri](https://github.com/coopbri)! - - Fix native terminal 256-color support by preferring the `omni-terminal` terminfo (256 colors) over `xterm-omni-terminal` (8 colors)
  - Fix clipboard segfault on Wayland (copy/paste crashed the terminal) by disabling LTO which triggered undefined behavior in smithay-clipboard FFI

## 0.1.0

### Minor Changes

- [`a304ec1`](https://github.com/omnidotdev/terminal/commit/a304ec1d987b6748ae4c99dba8a6baac88dab7a1) Thanks [@coopbri](https://github.com/coopbri)! - Opening the aperture

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
  - Linux tarball distribution
  - Optimized release profile — LTO, symbol stripping, panic abort, single codegen unit
