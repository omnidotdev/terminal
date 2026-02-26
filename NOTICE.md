# NOTICE

Omni Terminal contains code derived from several open source projects.
This file provides attribution as required by their respective licenses.

## Alacritty

**License:** Apache-2.0
**Source:** <https://github.com/alacritty/alacritty>
**Copyright:** The Alacritty Project Contributors

Portions of the following files were originally taken from Alacritty:

- `terminal-backend/src/crosswords/mod.rs`
- `terminal-backend/src/crosswords/square.rs`
- `terminal-backend/src/crosswords/search.rs`
- `terminal-backend/src/crosswords/vi_mode.rs`
- `terminal-backend/src/crosswords/grid/storage.rs`
- `terminal-backend/src/ansi/control.rs`
- `terminal-backend/src/ansi/graphics.rs`
- `terminal-backend/src/ansi/sixel.rs`
- `terminal-backend/src/ansi/iterm2_image_protocol.rs`
- `frontends/omni-terminal/src/bindings/mod.rs`
- `frontends/omni-terminal/src/bindings/kitty_keyboard.rs`
- `frontends/omni-terminal/src/screen/mod.rs`
- `frontends/omni-terminal/src/screen/touch.rs`
- `frontends/omni-terminal/src/cli.rs`
- `frontends/omni-terminal/src/panic.rs`
- `frontends/omni-terminal/src/scheduler.rs`
- `frontends/omni-terminal/src/platform/macos/mod.rs`
- `teletypewriter/src/unix/mod.rs`
- `teletypewriter/src/unix/macos.rs`

## Rio Terminal

**License:** MIT
**Source:** <https://github.com/raphamorim/rio>
**Copyright:** Raphael Amorim

Omni Terminal was originally forked from Rio Terminal. The `terminal-window`
and `corcovado` crates were adapted via Rio.

## Alacritty VTE (copa crate)

**License:** Apache-2.0 OR MIT
**Source:** <https://github.com/alacritty/vte>
**Copyright:** The Alacritty Project Contributors

The `copa` crate is a fork of Alacritty's VTE parser, extending Paul
Williams' ANSI parser state machine with custom instructions.

## winit (terminal-window crate)

**License:** Apache-2.0
**Source:** <https://github.com/rust-windowing/winit>
**Copyright:** The winit contributors

The `terminal-window` crate is a fork of winit, adapted via Rio Terminal.

## mio 0.6 (corcovado crate)

**License:** MIT
**Source:** <https://github.com/tokio-rs/mio>
**Copyright:** Carl Lerche and other contributors

The `corcovado` crate is a fork of mio 0.6, with additions from
mio-signal-hook and mio-extras, adapted via Rio Terminal.

## W3C UI Events

**License:** W3C Software and Document License
**Source:** <https://www.w3.org/TR/2017/CR-uievents-key-20170601/>
**Copyright:** 2017 W3C (MIT, ERCIM, Keio, Beihang)

Type definitions in `terminal-window/src/keyboard.rs` are derived from the
W3C UI Events KeyboardEvent key Values and code Values specifications.
