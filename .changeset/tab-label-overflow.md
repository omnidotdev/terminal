---
"@omnidotdev/terminal": patch
---

Fix long tab names overflowing into the next tab. Labels are now truncated to the tab's measured pixel width (rather than a fixed character count) and ellipsized, and the numeric/active-marker prefix is counted against the same budget so it can no longer push the name past the tab edge.
