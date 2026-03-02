---
"@omnidotdev/terminal": minor
---

Add `omni-terminal serve` subcommand that starts a WebSocket PTY server with the WASM frontend embedded in the binary. Defaults to HTTPS with auto-generated self-signed certificate on 127.0.0.1:3000. Behind a default-on `serve` Cargo feature flag. Removes the standalone `web-server` crate.
