---
"@omnidotdev/terminal": minor
---

Improve tab renaming and titles across both frontends. Tab names now derive from the shell's reported title (OSC 0/1/2) with the tab number rendered as a separate element instead of baked into the name, and a manual rename pins the title until cleared. Renaming supports clipboard paste and readline-style editing (Ctrl+U/K/W/A/E, arrows, Home/End) via a shared editor. The WASM frontend gains inline rename (double-click, Ctrl+Shift+L, right-click) with the tab title now updating live from the shell; the native frontend gains paste and readline editing in its rename prompt plus double-click to rename.
