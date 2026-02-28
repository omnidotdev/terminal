<div align="center">
  <img src="/assets/logo.png" width="100" />

  <h1 align="center">Omni Terminal</h1>

[Website](https://terminal.omni.dev) | [Docs](https://docs.omni.dev/armory/omni-terminal) | [Feedback](https://backfeed.omni.dev/workspaces/omni/projects/terminal) | [Discord](https://discord.gg/omnidotdev) | [X](https://x.com/omnidotdev)

</div>

**Omni Terminal** is a GPU-accelerated terminal emulator built to run everywhere.

> [!NOTE]
> Omni Terminal was originally forked from [Rio Terminal](https://github.com/raphamorim/rio)
> by [Raphael Amorim](https://github.com/raphamorim), licensed under MIT.
> We are grateful for his foundational work. Please consider
> [sponsoring him](https://github.com/sponsors/raphamorim) to support his
> continued open source contributions.

## Platforms

| Platform | Status |
| --- | --- |
| macOS | Experimental |
| Linux | Experimental |
| Windows | Experimental |
| Web (WebAssembly) | Experimental |
| Android | Experimental |

## Installation

| Platform | Channel | Command / Link |
| --- | --- | --- |
| All | [GitHub Releases](https://github.com/omnidotdev/terminal/releases) | Download from releases page |
| All | Source | `cargo install --git https://github.com/omnidotdev/terminal -p omni-terminal` |
| macOS | [Homebrew](https://brew.sh) | `brew install --cask omnidotdev/tap/omni-terminal` |
| Arch Linux | [AUR](https://aur.archlinux.org) | `yay -S omnidotdev-terminal` or `yay -S omnidotdev-terminal-bin` |
| Debian/Ubuntu | .deb | Download x11 or wayland .deb from releases |

### Build from source

```bash
# Linux: install system dependencies first
# Debian/Ubuntu: sudo apt install libwayland-dev libxkbcommon-dev libfontconfig-dev
# Arch: sudo pacman -S fontconfig freetype2 libxkbcommon wayland

git clone https://github.com/omnidotdev/terminal
cd terminal
cargo build --release -p omni-terminal
```

## Configuration

Configuration documentation is available at [terminal.omni.dev](https://terminal.omni.dev).

## Contributing

See Omni's [contributing docs](https://docs.omni.dev/contributing/overview).

## Ecosystem

- **[Omni CLI](https://github.com/omnidotdev/cli)**: Agentic CLI for the Omni ecosystem
- **[Beacon](https://github.com/omnidotdev/beacon-gateway)**: Voice and messaging gateway for AI assistants

## License

The code in this repository is licensed under MIT, &copy; [Omni LLC](https://omni.dev). See [LICENSE.md](LICENSE.md) for more information.
