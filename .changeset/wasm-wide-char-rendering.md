---
"@omnidotdev/terminal": patch
---

Fix wide characters (emoji, CJK) in the WASM terminal. They previously advanced the cursor by only one column, desyncing the grid from the shell and mangling the prompt when an emoji was pasted or echoed. Wide characters now occupy two columns (a leading cell plus a spacer), the renderer sizes them correctly and skips the spacer, and selection copy skips it too.
