# `runa - rn`

<div align="center">

[![Build](https://img.shields.io/github/actions/workflow/status/runa-fm/runa/rust.yml?style=flat-square&logo=github&label=Build)](https://github.com/runa-fm/runa/actions/workflows/rust.yml)
[![Documentation](https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square)](https://runa-fm.github.io/runa/)
[![Latest Release](https://img.shields.io/github/v/release/runa-fm/runa?style=flat-square&color=8839ef&label=Release)](https://github.com/runa-fm/runa/releases)
[![Crates.io](https://img.shields.io/crates/v/runa-fm?style=flat-square&color=e67e22&logo=rust)](https://crates.io/crates/runa-fm)
[![AUR](https://img.shields.io/aur/version/runa?label=AUR&color=blue&style=flat-square&logo=archlinux)](https://aur.archlinux.org/packages/runa)
[![COPR](https://img.shields.io/badge/COPR-runa-51A2DA?style=flat-square&logo=fedora)](https://copr.fedorainfracloud.org/coprs/alexm-dev/runa/)
[![Language](https://img.shields.io/github/languages/top/runa-fm/runa?style=flat-square&logo=rust&color=%23e67e22&label=Rust)](https://github.com/runa-fm/runa)
[![License](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue?style=flat-square)](#license)

</div>
<div align="center">

> **A fast, lightweight, and extremely customizable terminal file manager carved in Rust.**

</div>
<div align="center">

<a href="https://runa-fm.github.io/runa/configuration.html">Configuration Guide</a> • 
<a href="#installation">Installation</a> • 
<a href="https://runa-fm.github.io/runa/config-reference/general.html">General Settings</a> • 
<a href="https://runa-fm.github.io/runa/config-reference/display.html">Display Settings</a> • 
<a href="https://runa-fm.github.io/runa/config-reference/theme.html">Theme</a> • 
<a href="https://runa-fm.github.io/runa/config-reference/keys.html">Key Bindings</a>

</div>

- **Rich File Actions:** Move, filter, copy, rename, tabs, and more, all with the keyboard.
- **Extremely Customizable:** Tweak keybindings, colors, and layout via TOML.
- **Blazing Fast:** Instant navigation, even in huge directory trees.
- **Minimal Dependencies:** Only the essential crates; advanced features are opt-in.
- **Cross-Platform:** Works on Linux, macOS, and Windows.
- **Keyboard-Driven:** Every action accessible by keybindings. No mouse needed.
- **Integrated Find (`fd` required):** Blazingly fast file search.

<img src="https://github.com/user-attachments/assets/b02a5190-3e0a-45d3-ace1-f1d067f4bdaa" alt="runa" width="100%" style="max-width: 1274px; height: auto;" />
<br></br>

**runa - rn** is a feature-rich terminal file manager with a concurrent, multi-threaded architecture designed for speed and a minimal footprint.  
It allows you to navigate directories, view file listings, and perform typical file manager actions.  

<details>

<summary><strong>What makes runa fast?</strong></summary>

- **Multi-threaded engine:** Spawns 12 lightweight dedicated worker threads for I/O, preview, find, and file operations, so UI is never blocked.
- **Essential-only Rust crates:** No external TUI frameworks or bloat.
- **Direct terminal rendering:** Uses [ratatui](https://ratatui.rs/) & [crossterm](https://github.com/crossterm-rs/crossterm).
- **Optional blazing-fast find:** Integrates with [fd](https://github.com/sharkdp/fd) for recursive fuzzy search.
- **Blazingly lightweight:** The `rn` binary is compact (typically 1.9-2.2 MB, depending on operating system and architecture).
- **Minimal memory footprint**: Optimized for low RAM usage.

</details>

**runa** is very customizable, check out the [Configuration](https://runa-fm.github.io/runa/) docs for all the available options.

> [!NOTE]
> **runa** is stable for daily use and actively developed, with new releases expanding its features while keeping it fast and lightweight.  
> Configuration options may still change between releases.

## Changelog
For a detailed list of changes and release notes, see [CHANGELOG.md](./CHANGELOG.md).

## Installation

### Cargo:

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

> [!TIP]
> **Checksum & Attestation Check:**  
> You can verify the integrity of the release archives using the `SHA256SUMS.txt` file in the [Release](https://github.com/runa-fm/runa/releases) page.
>
> **Unix**
> To check every downloaded archive in the current folder against SHA256SUMS.txt:
> ```bash
> sha256sum -c --ignore-missing SHA256SUMS.txt
> ```
>
> **Windows**
> ```powershell
> Get-FileHash runa-windows-x86_64.zip -Algorithm SHA256
> ```
> Compare the output with the corresponding entry in `SHA256SUMS.txt`.
>
> You can verify the integrity of the release archives using the attestation verification with the GitHub CLI:
>
> ```bash
> gh attestation verify --repo runa-fm/runa <file_name>
> ```
> _Attestations for releases published before the repository moved to `runa-fm/runa` may not be available through the new repository._

### Build from source

Clone the repo and build with Cargo:

```bash
git clone https://github.com/runa-fm/runa.git
cd runa
cargo build --release
```

### Usage

After installation, start runa with: `rn`

> [!TIP]
> **Icons** (for files, folders, etc.) are disabled by default, but can be enabled in your `runa.toml`.  
> To display them correctly, use a **Nerd Font** or a patched font in your terminal.  
> Without a Nerd Font, icons may appear incorrectly and the UI may not render as intended.  

## Optional Enhancements

`runa` is designed to be lightweight and standalone. However, some advanced features leverage specialized external tools:

* **Fuzzy Search:** To enable fast, recursive fuzzy finding, install **[fd](https://github.com/sharkdp/fd)**.
  * If `fd` is detected in your `PATH`, the search feature will be enabled automatically.
  * Without it, `runa` remains a fully functional file manager but will notify you if you attempt a recursive search.
  * Also enables `move_file` auto-complete if `fd` is enabled.

* **Preview Syntax coloring**: To enable syntax coloring in the preview pane, install **[bat](https://github.com/sharkdp/bat)**
  * If `bat` is detected and installed, you can switch method in the runa.toml to `method = "bat"`.
  * Without it, `runa` uses the `internal` preview method, which is a plain preview method useful for extra speed without syntax highlighting.

## Configuration

By default, runa checks for configuration in these locations, in order:

1. If the environment variable `RUNA_CONFIG` is set, its value is used as the config path.
2. If `XDG_CONFIG_HOME` is set, runa looks for:  
    ```sh
    $XDG_CONFIG_HOME/runa/runa.toml
    ```

3. Otherwise, the fallback is:  
    ```sh
    $HOME/.config/runa/runa.toml
    ```

    (On Windows, this means inside the user's config folder.)

#### Overriding the config path

You can explicitly set a custom config location by setting the environment variable `RUNA_CONFIG`.

**Example:**  
```sh
export RUNA_CONFIG=/path/to/your/runa.toml
```

You can generate a default config using the --init or --init-full flag:

```bash
rn --init

# For the whole configuration options runa.toml
rn --init-full

# For help with all the configuration options.
rn --config-help
```

This will generate a config in the default config path.

## Roadmap

### Planned Features

- [ ] Image Previews: Support for Sixel/Kitty graphics protocols

### Completed Features

- [x] Tabs: Spawn and manage multiple tabs of runa
- [x] Syntax Highlighting: `bat` integration for the preview pane  
- [x] Search & Discovery: Integrated fuzzy finding (`fd` support)  
- [x] File Operations: Copy, move, delete, and rename from within the UI  
- [x] Content Search: Text search and filtering  
- [x] Performance: Reactive rendering  
- [x] UI Customization: Pane-specific styling and Hex color support  
- [x] Navigation Context: Persistent Parent (Origin) and Preview panes  

> See [CHANGELOG.md](./CHANGELOG.md) for version details and release history.

---
<img src="https://github.com/user-attachments/assets/690bf317-57cd-4cb2-91c0-e293941c1281" alt="runa_alt" style="max-width: 100%; height: auto;" />

---

## Support & Contribute
If you enjoy using **runa**, you can help the project grow:

* ⭐ **Star the Repo:** It helps more people discover runa :)
* 🐛 **Report Bugs:** Open an issue if something doesn't work as expected.
* 💡 **Feature Requests:** Suggest new ideas in the [Discussions](https://github.com/runa-fm/runa/discussions) tab.
* 🦀 **Contribute:** Pull requests are always welcome! Check out the [CONTRIBUTING guide](CONTRIBUTING.md) for more info.


## Special Thanks
Many thanks to [@lmartinez-mirror](https://github.com/lmartinez-mirror), the maintainer of [runa-bin](https://aur.archlinux.org/packages/runa-bin), for maintaining the binary AUR package.

Many thanks to [@sharkdp](https://github.com/sharkdp) for `fd` and `bat`, excellent CLI tools runa integrates with for fuzzy finding and syntax preview.

## License
This project is dual licensed under the MIT OR Apache-2.0 license.  

See the [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) files for details.
