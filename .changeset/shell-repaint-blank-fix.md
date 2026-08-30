---
"@omnidotdev/terminal": patch
---

Repaint the active tab after a resize so the terminal is not left blank after a hidden-then-shown transition (e.g. an embedding page switching tabs away and back), and disable `wasm-opt` in the production build because `wasm-opt -O` (Binaryen v123) produced a module that validated but rendered blank at runtime for the wgpu path.
