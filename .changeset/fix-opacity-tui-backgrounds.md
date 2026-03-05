---
"@omnidotdev/terminal": patch
---

fix(renderer): uniform opacity and background color for TUI programs

- Add dedicated replace-blend GPU pipeline for background rects to prevent alpha accumulation when drawing semi-transparent backgrounds over a semi-transparent clear color
- Track background vertices separately in the compositor so cell backgrounds render with correct opacity
- Force full damage on all contexts when opacity changes via keybinding so all cells re-render
- Preserve current opacity when changing background color via OSC 11
- Process background state changes before cell rendering so cells use the updated default background reference
- Treat cells matching the original theme background as "default" so TUI programs that set explicit backgrounds still track OSC 11 background changes
