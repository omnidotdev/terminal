---
"@omnidotdev/terminal": patch
---

Fix a crash when hovering a URL on a line that also contains multi-byte characters while the hint modifier (Alt on Linux) is held. The regex-based hyperlink hint matched URLs by byte offset into the extracted line text but indexed the grid by column, so any multi-byte character on the line (emoji, CJK, or powerline/nerd-font prompt glyphs) pushed the offset past the real column and panicked with an out-of-bounds grid index, taking down the window. Regex match offsets are now converted from byte offsets to column indices in both the hover highlight path and the hint-mode match collection, so the reported columns always stay within the grid.
