# Key Bindings

Keyboard shortcuts and keybind configuration.

## Overview

All actions in runa are configurable via the `[keys]` section. All values are lists supporting multiple shortcuts per action.

## Modifier Syntax

For keys involving Ctrl, Alt/Meta or Shift, use the following syntax:

| Modifier | Bracketed              | Standard                | Result    |
|----------|------------------------|-------------------------|-----------|
| Control  | `<c-key>`              | `ctrl+key`              | Ctrl+Key  |
| Alt/Meta | `<a-key>` or `<m-key>` | `alt+key` or `meta+key` | Alt+Key   |
| Shift    | `<s-key>`              | `shift+key`             | Shift+Key |

## Key Names

| Key        | Names                           |
|------------|---------------------------------|
| Arrows     | `up`, `down`, `left`, `right`   |
| Enter      | `enter`                         |
| Escape     | `esc`                           |
| Backspace  | `backspace`, `back`             |
| Tab        | `tab`                           |
| Space      | `space`, `spc`, `" "`           |
| Page Up    | `pgup`, `pageup`, `page_up`     |
| Page Down  | `pgdn`, `pagedown`, `page_down` |
| Home / End | `home`, `end`                   |
| F-keys     | `f1` … `f12`                    |

Names are case-insensitive and also work with modifiers, e.g. `<c-pgup>`. Any other single character binds that character, e.g. `"g"` or `"?"`.

## File Operations

### `open_file`

- **Default**: `["enter"]`

Open the selected file or enter directory.

### `delete`

- **Default**: `["d"]`

Delete file (behavior controlled by `[general] move_to_trash`).

### `alternate_delete`

- **Default**: `["<m-d>"]`

Alternates between move_to_trash and permanently delete.

### `copy`

- **Default**: `["y"]`

Copy file to clipboard.

### `paste`

- **Default**: `["p"]`

Paste file from clipboard.

### `rename`

- **Default**: `["r"]`

Rename file.

### `create`

- **Default**: `["n"]`

Create new file.

### `create_directory`

- **Default**: `["N"]`

Create new directory.

### `move_file`

- **Default**: `["m"]`

Move file.

## Navigation

### `go_up`

- **Default**: `["k", "up"]`

Move selection up.

### `go_down`

- **Default**: `["j", "down"]`

Move selection down.

### `go_parent`

- **Default**: `["h", "left", "back"]`

Go to parent directory.

### `go_into_dir`

- **Default**: `["l", "right"]`

Enter directory.

### `go_to_bottom`

- **Default**: `["G"]`

Jump to last entry.

## Go-To Actions (Prefix-based)

These actions are triggered by pressing the `prefix_go_to` key, then another key.

**Example**: Press "g" then "p" for `go_to_path`

### `prefix_go_to`

- **Default**: `["g"]`

Prefix key for go-to actions.

### `go_to_top`

- **Default**: `["g"]`

Jump to first entry.

### `go_to_home`

- **Default**: `["h"]`

Go to home directory.

### `go_to_path`

- **Default**: `["p"]`

Go to specific path (opens input dialog).

### `go_to_trash`

- **Default**: `["t"]`

Open the virtual Trash view (`trash:/`). Supported on Linux and Windows; macOS displays an unsupported notice.

Inside the Trash view:
- `r` — Restore selected or marked items to their original location
- `d` / `<m-d>` (uses `delete` / `alternate_delete`) — Permanently delete selected or marked items (with confirmation)
- `E` — Empty trash (with confirmation)
- `h` / `Left` (uses `go_parent`) — Return to the previous directory (only when opened via `g t`; startup Trash tabs stay in Trash until navigated away with `g h` / `g p`)
- Standard navigation (`go_up`, `go_down`, `g g`, `go_to_bottom`), markers (`toggle_marker`, `select_all`, `clear_markers`), tabs, and `reload` work as in the main pane.

## View and Display

### `filter`

- **Default**: `["f"]`

Filter entries by pattern.

### `find`

- **Default**: `["s"]`

Search/find files (requires `fd` for best performance).

### `show_info`

- **Default**: `["i"]`

Show detailed file information widget.

## Selection and Markers

### `toggle_marker`

- **Default**: `["<space>"]`

Toggle marker on current entry (multi-select).

### `select_all`

- **Default**: `["<c-a>"]`

Select all entries in current directory.

### `clear_markers`

- **Default**: `["<c-c>"]`

Clear all markers.

## Filtering and Cleanup

### `clear_filter`

- **Default**: `["<c-f>"]`

Clear active filter.

### `clear_clipboard`

- **Default**: `["<f2>"]`

Clear clipboard.

### `clear_all`

- **Default**: `["<c-l>"]`

Clear all (filter, clipboard, markers).

## Sorting

Sorting actions are triggered by pressing the `sort` key, then another key.

**Example**: Press "o" then "e" for `sort_by_extension`

### `sort`

- **Default**: `["o"]`

Prefix key to trigger sorting.

### `sort_by_name`

- **Default**: `["n"]`

Sort by filename.

### `sort_by_natural`

- **Default**: `["N"]`

Sort by natural order.

### `sort_by_extension`

- **Default**: `["e"]`

Sort by file extension.

### `sort_by_size`

- **Default**: `["s"]`

Sort by file size.

### `sort_by_modified`

- **Default**: `["m"]`

Sort by modified date.

### `sort_by_created`

- **Default**: `["c"]`

Sort by created date.

### `sort_by_accessed`

- **Default**: `["a"]`

Sort by accessed date.

## Tab Management

### `tab_new`

- **Default**: `["<c-t>"]`

Create new tab.

### `tab_close`

- **Default**: `["<c-w>"]`

Close current tab.

### `tab_next`

- **Default**: `["<c-n>"]`

Switch to next tab.

### `tab_prev`

- **Default**: `["<c-p>"]`

Switch to previous tab.

## Widget Interaction

### `scroll_up`

- **Default**: `["pgup"]`

Scrolls up in the preview pane, for files and directories, and in widgets.

### `scroll_down`

- **Default**: `["pgdn"]`

Scrolls down in the preview pane, for files and directories, and in widgets.

## General

### `quit`

- **Default**: `["q", "esc"]`

Quit runa.

### `reload`

- **Default**: `["<c-r>"]`

Reload the UI.

### `keybind_help`

- **Default**: `["?"]`

Show keybinding help dialog.


## Configuration Notes

- Multiple keybinds per action are supported
- You can use `" "` for space key
- Single keybind instead of array is also possible:
  ```toml
  sort = "o"
  find = "s"
  ```
- You may remove any binding to let it fall back to the default
- go_to_* actions are triggered by pressing the "g" prefix, then another key
- sort_by_* actions are triggered by pressing the "o" (sort) prefix, then the sort key

