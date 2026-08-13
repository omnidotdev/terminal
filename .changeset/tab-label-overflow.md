---
"@omnidotdev/terminal": patch
---

Fix long tab names overflowing into the next tab. Labels are now truncated to the tab's measured pixel width (rather than a fixed character count) and ellipsized, and the numeric prefix is counted against the same budget so it can no longer push the name past the tab edge.

Every tab now shows a numeric prefix, including the active one, so prefix widths are uniform across the tab bar. The active tab is distinguished by its color and highlight underline rather than a separate marker glyph.
