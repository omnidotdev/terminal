---
"@omnidotdev/terminal": patch
---

- Fix native terminal 256-color support by preferring the `omni-terminal` terminfo (256 colors) over `xterm-omni-terminal` (8 colors)
- Fix clipboard segfault on Wayland (copy/paste crashed the terminal) by disabling LTO which triggered undefined behavior in smithay-clipboard FFI
