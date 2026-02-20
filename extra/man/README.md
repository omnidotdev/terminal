# Omni Terminal Man Pages

This directory contains manual pages for Omni Terminal emulator in scdoc format.

## Files

- `omni-terminal.1.scd` - Main Omni Terminal manual page (section 1)
- `omni-terminal.5.scd` - Omni Terminal configuration file format manual page (section 5)
- `omni-terminal-bindings.5.scd` - Omni Terminal key bindings manual page (section 5)

## Building

To build the man pages, you need `scdoc` installed:

### Install scdoc

**macOS (Homebrew):**
```bash
brew install scdoc
```

**Ubuntu/Debian:**
```bash
sudo apt install scdoc
```

**Arch Linux:**
```bash
sudo pacman -S scdoc
```

**From source:**
```bash
git clone https://git.sr.ht/~sircmpwn/scdoc
cd scdoc
make
sudo make install
```

### Build man pages

```bash
just man-pages
```

### Install man pages

```bash
# Install to system man directory (requires sudo)
sudo just man-install

# Update man database
sudo mandb
```

### View man pages

```bash
man omni-terminal
man 5 omni-terminal
man 5 omni-terminal-bindings
```

## Format

The man pages are written in scdoc format, which is a simple markup language for writing man pages. See the [scdoc documentation](https://git.sr.ht/~sircmpwn/scdoc) for syntax details.
