---
"@omnidotdev/terminal": patch
---

fix(wasm): process exit UX and iOS Safari rendering

- Show "[Process exited. Press Enter to restart.]" when shell exits instead of leaving a frozen terminal
- Restart session on Enter after process exit (desktop keydown + mobile input)
- Server sends `exited` WebSocket message when PTY reader detects shell exit (EIO/EOF)
- Fix iOS Safari rendering by forcing WebGL backend (Safari's WebGPU has device-loss issues during glyph rendering)
- Enable wgpu `webgl` feature for WebGL2 fallback support on WASM targets
- Detect iOS/iPadOS via user agent and maxTouchPoints for backend selection
- Fix FullRender cache path advancing by one cell width instead of full run width
