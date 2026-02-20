OMNI-TERMINAL(1)

# NAME

Omni Terminal - A cross-platform, GPU-accelerated terminal emulator.

# SYNOPSIS

Omni Terminal is a modern terminal emulator built with Rust and WebGPU that provides high performance rendering with support for modern terminal features including sixel graphics, hyperlinks, and custom shaders.

# FLAGS

*-h, --help*

	Prints help information.

*-V, --version*

	Prints version information.

# OPTIONS

*-e, --command* _<COMMAND>..._

	Command and args to execute (must be last argument).

*-w, --working-dir* _<WORKING_DIR>_

	Start the shell in the specified working directory.

*--write-config* _[<PATH>]_

	Writes the config to a given path or the default location.

	Omni Terminal doesn't create the config file for you, but it looks for one in the
	following locations on UNIX systems:

		. _$OMNI_TERMINAL_CONFIG_HOME/config.toml_
		. _$XDG_CONFIG_HOME/omni/terminal/config.toml_
		. _$HOME/.config/omni/terminal/config.toml_

	On Windows, the config file will be looked for in:

		. _%USERPROFILE%\\AppData\\Local\\omni\\terminal\\config.toml_

*--enable-log-file*

	Writes the logs to a file inside the config directory.

*--title-placeholder* _<TITLE>_

	Start window with specified title.

# CONFIGURATION

Omni Terminal uses TOML format for configuration. The configuration file is automatically
reloaded when changed. See *omni-terminal*(5) for detailed configuration documentation.

# FEATURES

Omni Terminal supports many modern terminal features:

- GPU-accelerated rendering with WebGPU
- Sixel graphics protocol
- iTerm2 image protocol
- Kitty keyboard protocol
- Hyperlinks (OSC 8)
- Vi mode for navigation
- Split panels
- Custom shaders (RetroArch compatible)
- Adaptive themes
- Font ligatures
- Wide color gamut support

# ENVIRONMENT

*OMNI_TERMINAL_CONFIG_HOME*

	Custom path for Omni Terminal configuration directory. When set, Omni Terminal will look for
	configuration files in this directory instead of the default locations.

# SEE ALSO

*omni-terminal*(5)

# BUGS

Found a bug? Please report it at _https://github.com/omnidotdev/terminal/issues_.

# MAINTAINERS

- Omni LLC <hello@omni.dev>
