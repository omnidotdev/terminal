---
"@omnidotdev/terminal": patch
---

Fix Backspace clearing the whole name when renaming a tab. The rename field opens with the existing name selected so typing replaces it, but the selection is not drawn, so Backspace wiping everything was confusing. Backspace now drops the selection and deletes a single character; typing or pasting still replaces the name, and Ctrl+U still clears it.
