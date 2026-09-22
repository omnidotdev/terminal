---
"@omnidotdev/terminal": patch
---

Add a Reset Terminal action to recover from a crashed or killed program that left mouse tracking, bracketed paste, or the alternate screen enabled (for example, escape sequences printing on mouse movement at the shell prompt). It clears those modes without touching scrollback or vi mode, is bound to Cmd+Shift+R on macOS and Ctrl+Alt+R elsewhere, and is configurable via the "resetterminal" keybinding action.
