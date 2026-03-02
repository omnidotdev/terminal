---
"@omnidotdev/terminal": patch
---

Switch release profile from `panic = "abort"` to `panic = "unwind"`, fixing Ctrl+V segfault (stack smashing) on Wayland
