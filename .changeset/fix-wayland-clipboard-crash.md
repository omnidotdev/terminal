---
"@omnidotdev/terminal": patch
---

Replace smithay-clipboard with wl-paste/wl-copy subprocess calls, fixing Ctrl+V crash on Wayland caused by internal `.unwrap()` panics under `panic = "abort"`
