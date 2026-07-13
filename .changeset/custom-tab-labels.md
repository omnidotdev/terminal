---
"@omnidotdev/terminal": minor
---

Add custom tab labels. Press `Ctrl+Shift+L` (`Cmd+Shift+L` on macOS) or bind the new `renametab` action to open a prompt and give the current tab a human-readable name. A custom label overrides the auto-derived title template and is not overwritten by process, working-directory, or OSC title changes. Committing an empty value clears the label and reverts the tab to its automatic title.

In the `TopTab`/`BottomTab` navigation modes, labeling a single tab reveals the tab bar so the name is visible in-window even with `hide-if-single` enabled; clearing the label hides it again. Tab-name rendering is now truncated by character rather than byte to avoid panicking on multibyte (unicode) names.

Labels are per-pane: `renameTab` names the focused pane. The focused pane's label drives the tab title, and in a split each labeled pane reserves a one-line title strip at its top showing the name, so every pane name is visible at once without overlapping content.
