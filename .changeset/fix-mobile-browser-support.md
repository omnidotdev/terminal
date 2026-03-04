---
"@omnidotdev/terminal": patch
---

fix(wasm): mobile browser support for remote terminal access

- Fix terminal text wrapping at wrong column on high-DPI devices by removing DPR double-counting in cell dimension calculation
- Fix Android virtual keyboard not appearing by adding touchend focus handler and forwarding textarea input events to the PTY
- Fix iOS rendering by gracefully handling sugarloaf font warnings instead of discarding the working GPU instance
- Add visible panic overlay for mobile debugging (no console access on mobile browsers)
- Make hidden textarea full-size with proper styling to ensure mobile browsers treat it as focusable
