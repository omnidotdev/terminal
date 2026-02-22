---
"@omnidotdev/terminal": minor
---

Initial release of embeddable WebGPU terminal component

- Terminal.init(container, options) async factory API
- WebSocket PTY connection with auto-reconnect and session persistence
- Multi-session tabs (Ctrl+T/Ctrl+W)
- Sugarloaf WebGPU rendering via WASM
- Copa ANSI parser with SGR colors, scroll regions, mouse reporting
- IME support for international input
- Bracketed paste support
