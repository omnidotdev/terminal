---
"@omnidotdev/terminal": patch
---

Fix terminal query replies leaking into the wrong tab. A program's reply to a terminal query was written to whichever tab was focused rather than to the terminal that issued the query, so running a program in one tab could dump bytes such as "OmniTerminal 0.4.3", the keyboard-mode report, and the device-attributes report into another tab's prompt. Every reply now carries the originating route id and is delivered to that terminal's PTY, including when the tab is in the background. This covers Primary and Secondary Device Attributes, XTVERSION, Kitty keyboard mode, device-status and mode reports, color (OSC) queries, text-area size reports, and OSC 52 clipboard paste-back.
