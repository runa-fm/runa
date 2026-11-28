# General Settings

Configuration options that control runa's general behavior and file visibility.

## Overview

The `[general]` section contains settings for file display, sorting, search behavior, and startup configuration.

Any unused or default options can be removed from `runa.toml`. Omitted options will use internal defaults.

## Options

### `dirs_first`

- **Type**: `boolean`
- **Default**: `true`

Sort directories before files in the main pane. When enabled, directories always appear first, helping you navigate faster without hunting through mixed listings. Sorting methods (natural, name, size, etc.) still apply within directories and files groups separately.

### `show_hidden`

- **Type**: `boolean`
- **Default**: `true`

Show hidden files and directories. On Linux/macOS, displays files starting with `.` (e.g., `.config`, `.ssh`). On Windows, see `show_system` for system file visibility.

**Related**: Use `always_show` to force specific directories visible even when `show_hidden = false`.

### `show_symlink`

- **Type**: `boolean`
- **Default**: `true`

Show symbolic links and their target paths in the main pane.

### `show_system`

- **Type**: `boolean`
- **Default**: `false`

Show hidden system files. Mostly relevant on Windows. When disabled, system files remain hidden even if `show_hidden = true`.

### `case_insensitive`

- **Type**: `boolean`
- **Default**: `true`

Ignore case sensitivity when searching and sorting. When enabled, `README`, `readme`, and `ReadMe` are treated as equivalent during sort operations and searches.

### `always_show`

- **Type**: `array of strings`
- **Default**: `[]`

List of directory names that should always be visible, even when `show_hidden = false`. Useful for forcing commonly-used hidden directories to always appear.

**Example**:
```toml
[general]
always_show = [".config", ".ssh", "Downloads"]
```

### `max_find_results`

- **Type**: `integer`
- **Default**: `20000`
- **Constraints**: Minimum `15`, Maximum `1,000,000` (values above are clamped)

Configure the maximum number of find/search results to display. Higher values increase memory usage but show more results. Very large values (100k+) may cause UI lag when scrolling results.

### `move_to_trash`

- **Type**: `boolean`
- **Default**: `true`

When enabled, deleted files are moved to the recycle bin instead of being permanently deleted. This setting controls the default delete action; the `alternate_delete` keybind alternates between the two behaviors on a per-delete basis.

## Startup Configuration

### `[general.startup]`

Configure which tabs open when runa starts.

#### `tabs`

- **Type**: `array of strings`
- **Default**: `[]` (unconfigured)

List of paths that open as tabs on startup. Supports up to 9 tabs. Tab placement is determined by the order in the list.

**Special values**:
- `"cwd"` or `"."` — Opens at the current working directory
- `"trash"` or `"Trash"` — Opens a virtual Trash tab (`trash:/`; Linux and Windows only)

**Example**:
```toml
[general.startup]
tabs = ["cwd", "~/Downloads", "~/Projects", "trash"]
```

**Notes**:
- On Windows, use single quotes for paths to avoid backslash escaping: `'C:\Users\...'`
- With double quotes, escape backslashes: `"C:\\Users\\..."`
- Invalid or missing paths in `startup.tabs` are skipped with a warning; if no tab can be opened, runa falls back to the current directory
- Omitting this section starts runa normally at the current directory
- You can also start multiple tabs from the CLI: `rn ~/path1 ~/path2 ~/path3` (including `rn trash`)
