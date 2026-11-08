# Runa Configuration Guide

runa is configured via a TOML file. By default, it looks for your config in the following order:

1. **Environment Variable**: `RUNA_CONFIG` - If set, runa uses this path as the config file
   ```sh
   export RUNA_CONFIG=/path/to/runa.toml
   ```

2. **XDG Config Directory** (Linux, macOS, and supported environments):
   ```sh
   $XDG_CONFIG_HOME/runa/runa.toml
   ```

3. **Default Path**:
   - `~/.config/runa/runa.toml` (Linux/macOS)
   - `C:\Users\<UserName>\.config\runa\runa.toml` (Windows)

> runa is under active development and configuration options may change over time.

## Installation

### Cargo

You can install runa from [crates.io](https://crates.io/crates/runa-fm)

```bash
cargo install runa-fm
```

### Arch Linux (AUR)

You can install runa from the [AUR](https://aur.archlinux.org/packages/runa) using an AUR helper like `paru` or `yay`:

```bash
yay -S runa

# or for binaries through the AUR
yay -S runa-bin
```

### Fedora

You can install runa from [COPR](https://copr.fedorainfracloud.org/coprs/alexm-dev/runa/).

```bash
sudo dnf copr enable alexm-dev/runa
sudo dnf install runa
```

### Homebrew

You can install runa via homebrew.

```bash
brew tap runa-fm/tap
brew install runa
```

### Scoop (Windows)

You can install runa via scoop.

```bash
scoop bucket add runa-fm https://github.com/runa-fm/scoop-bucket
scoop install runa
```

### Pre-compiled Binaries

If you'd like to download Pre-compiled binaries instead of installing runa as a crate in cargo or via the AUR,
you can grab the latest binaries for Linux, Windows and macOS from the [Release](https://github.com/runa-fm/runa/releases) page.

After downloading, add the `rn` (Linux/macOS) or `rn.exe` (Windows) binary to your system `PATH` to use runa from your terminal.

### Build from source

Clone the repo and build with Cargo:

```bash
git clone https://github.com/runa-fm/runa.git
cd runa
cargo build --release
cargo install --path .
```

## Quick Start

If you don't have a config file yet, generate one:

- `rn --init` - Generates a minimal configuration
- `rn --init-full` - Generates a full configuration with all options and inline documentation

Other useful commands:

- `rn --help` - Shows the CLI help menu
- `rn --version` - Displays the current installed version
- `rn --config-help` - Displays documentation for configuration sections (e.g., `rn --config-help general`)

## Optional Tool Integration

Some features require external tools to be installed:

- **[bat](https://github.com/sharkdp/bat)** - Fast, syntax-highlighted file previews (optional, highly recommended)
- **[fd](https://github.com/sharkdp/fd)** - Blazing fast recursive file/folder search (optional, highly recommended)

Both tools are completely optional. runa falls back to built-in methods if they're not installed.

**Install with your package manager**:
```sh
sudo pacman -S bat fd           # Arch
sudo apt install bat fd-find    # Debian/Ubuntu
sudo dnf install bat fd-find    # Fedora/RHEL
brew install bat fd             # macOS
```

## Configuration Sections

Each section below links to detailed documentation for all available options:

### [General Settings](config-reference/general.md)

File visibility, sorting, search behavior, and startup tabs.

**Options**: `dirs_first`, `show_hidden`, `show_symlink`, `show_system`, `case_insensitive`, `always_show`, `max_find_results`, `move_to_trash`, `[general.startup]`

### [Display Settings](config-reference/display.md)

Pane layout, borders, previews, file information, and UI styling.

**Sections**: `[display]`, `[display.layout]`, `[display.preview_options]`, `[display.preview_options.archive]`, `[display.info]`, `[display.status]`

### [Theme Configuration](config-reference/theme.md)

Colors, icon coloring, styling, and widget customization.

**Sections**: `[theme]`, color overrides for all UI elements, `[theme.widget]`, `[theme.status_line]`, `[theme.info]`

### [Editor Configuration](config-reference/editor.md)

Editor program selection and per-extension/per-filename overrides.

**Section**: `[editor]`

### [Commands](config-reference/commands.md)

Own commands bound to keys, like `lazygit` or extracting an archive.

**Section**: `[commands]`

### [Key Bindings](config-reference/keys.md)

Keyboard shortcuts for all actions and navigation.

**Section**: `[keys]`

## Complete Reference

For the complete list of all options with defaults and inline documentation, see:

- `runa_full.toml` in [assets/config](https://github.com/runa-fm/runa/blob/main/assets/config/runa_full.toml)
- Run `rn --init-full` to generate a full config file with all options

## Examples

### Minimal Configuration

```toml
[general]
dirs_first = true
show_hidden = true

[display]
borders = "unified"

[editor]
default = "nvim"
```

### Custom Theme with Layout

```toml
[display]
borders = "split"

[display.layout]
main = 40

[theme]
name = "gruvbox-dark"
selection.fg = "#EBA"
entry.fg = "#333333"
directory.fg = "magenta"

[theme.accent]
fg = "#00ff00"

[theme.widget]
position = [25, 60]
size = { w = 36, h = 20 }
```

### Showcase

This is the setup shown in the README screenshot. It needs a Nerd Font and a terminal with truecolor support.

![runa setup](https://github.com/user-attachments/assets/b02a5190-3e0a-45d3-ace1-f1d067f4bdaa)

```toml
[general]
dirs_first = true
show_hidden = true
startup.tabs = ["cwd", '~']

[display]
borders = "unified"
titles = true
icons = true
parent = true
preview = true
preview_underline = false

[display.layout]
preview = 100

[display.preview_options]
method = "bat"
style = "numbers"

[display.info]
format = "{perms} | {size} | {mtime}"
date_format = "%d %b %y %H:%M"

[theme]
accent = "#8acccc"
path.bg = "#333333"
selection.bg = "#262626"
tab.active.fg = "#8ac"
tab.active.bg = "#555555"
tab.inactive.bg = "#444444"
tab.line_format = " {idx} {name} "
status_line.bg = "#333333"
status_line.fg = "#8ac"
widget.border.fg = "green"
info.date.bg = "#333333"
info.date.fg = "green"
info.size.bg = "#333333"
info.perms.bg = "#333333"
```

