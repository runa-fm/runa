# Theme Configuration

Color schemes, icon coloring, styling, and widget customization.

## Overview

The `[theme]` section controls all color settings in runa, including preset themes, color overrides, and widget styling.

## Color Format

Theme color values can be:
- Terminal color names: `"Red"`, `"Blue"`, `"Cyan"`, `"LightGreen"`, etc.
- Hex codes: `"#RRGGBB"` (e.g., `"#FF5733"`)
- Special value: `"default"` (uses terminal default)

### Color Field Syntax

Color fields support two formats:

**1. Flat shorthand (sets the foreground)**:
```toml
accent = "blue"
```

**2. Explicit form (foreground and background)**:
```toml
accent.fg = "blue"
accent.bg = "default"
```

Both are equivalent: `accent = "blue"` == `accent.fg = "blue"`

**Rules**:
- If only a single value is provided, it maps to `.fg`
- `.bg` defaults to `"default"` unless explicitly set
- `.fg` will always take precedence over the shorthand

## Main Theme Options

### `name`

- **Type**: `string`
- **Default**: `"default"`

The name of the preset themes included in runa. Choose a preset, or leave as "default" or omit.

**Available options (case-sensitive strings)**:
- `"gruvbox-dark"`
- `"gruvbox-dark-hard"`
- `"gruvbox-light"`
- `"catppuccin-mocha"`
- `"catppuccin-frappe"`
- `"catppuccin-macchiato"`
- `"catppuccin-latte"`
- `"nightfox"`
- `"carbonfox"`
- `"tokyonight"` (or `"tokyonight-night"`)
- `"tokyonight-storm"`
- `"tokyonight-day"`
- `"everforest"`
- `"rose-pine"` (or `"rose_pine"`)
- `"nord"`
- `"two-dark"`
- `"one-dark"`
- `"solarized-dark"`
- `"solarized-light"`
- `"dracula"`
- `"monokai"`

### `bg`

- **Type**: `string`
- **Default**: `"default"`

Background color of all panes. `[theme.parent]` and `[theme.preview]` can override it per pane.

### `selection_icon`

- **Type**: `string`
- **Default**: `""`

The symbol for the current selection. Use `""` or `" "` to disable or set `[display] selection_marker = false`.

### `exe_color`

- **Type**: `string`
- **Default**: `"LightGreen"`

Set the colors of binaries/executables.

## Color Overrides

You can set each color field directly in `[theme]` instead of creating each `[theme]` subsection for overriding and or creating custom themes.

**Example**:
```toml
[theme]
selection.fg = "#EEBBAA"
directory.fg = "#7BA"
widget.border.fg = "#555555"
symlink.directory = "#8aA"
```

## Pane Color Sections

### `[theme.selection]`

Selection bar colors. This is the global selection coloring section and can be overwritten for each pane.

- `fg` - Foreground color
- `bg` - Background color

### `[theme.accent]`

Borders and titles colors.

- `fg` - Foreground color
- `bg` - Background color

### `[theme.entry]`

Entry colors in all panes. Applies to the entry name and the default file and folder icons, not the whole row.  
Icons with their own color, such as `.json` or `.png`, keep their foreground color.

- `fg` - Foreground color
- `bg` - Background color

### `[theme.directory]`

Directory entry colors. Overrides `[theme.entry]` for directories.

- `fg` - Foreground color
- `bg` - Background color

### `[theme.separator]`

Vertical separators color.

- `fg` - Foreground color
- `bg` - Background color

### `[theme.parent]`

Parent pane overrides.

- `bg` - Overrides the `[theme]` `bg` for just the parent pane
- `entry.fg` - Overrides `[theme.entry]` for just the parent pane
- `entry.bg` - Overrides `[theme.entry]` for just the parent pane
- `selection_mode` - Sets the selection coloring mode. If `"off"`, then `selection.fg` and `.bg` are ignored. If `"on"`, then `selection.bg` and `.bg` are set to `"default"` or to a specific override.
- `selection.fg` - Overrides the central `[theme.selection]` for just the parent pane
- `selection.bg` - Overrides the central `[theme.selection]` for just the parent pane

A flat `fg` is still accepted as an older name for `entry.fg`.

### `[theme.preview]`

Preview pane overrides.

- `bg` - Overrides the `[theme]` `bg` for just the preview pane
- `entry.fg` - Overrides `[theme.entry]` for just the preview pane
- `entry.bg` - Overrides `[theme.entry]` for just the preview pane
- `selection_mode` - Sets the selection coloring mode. If `"off"`, then `selection.fg` and `.bg` are ignored. If `"on"`, then `selection.bg` and `.bg` are set to `"default"` or to a specific override.
- `selection.fg` - Overrides the central `[theme.selection]` for just the preview pane
- `selection.bg` - Overrides the central `[theme.selection]` for just the preview pane

A flat `fg` is still accepted as an older name for `entry.fg`.

### `[theme.symlink]`

Symlink color configuration.

- `directory` - Set the symlink colors for symlinks who are linked to directories
- `file` - Set color for symlinks who are linked to files
- `target` - Set the target path color of the symlink

## Entry and Icon Coloring

### `[theme.icon_color]`

Map icon colors to specific file extensions.

**Example**:
```toml
[theme.icon_color]
"rs" = "#dea584"
```

### `[theme.filename]`

Map entry colors to specific filenames (exact match).

**Example**:
```toml
[theme.filename]
"README.md" = { fg = "yellow", bg = "default" }
```

### `[theme.ext]`

Map entry color for specific file extensions.

**Example**:
```toml
[theme.ext]
"rs" = { fg = "default", bg = "default" }
```

## Additional UI Elements

### `[theme.marker]`

Multi-select marker colors and settings.

- `icon` - Marker icon symbol (set to `" "` to disable)
- `fg` - Foreground color
- `bg` - Background color
- `clipboard.fg` - Change the color of the clipboard when you copy an entry via multi-select or via normal yank/copy
- `clipboard.bg` - Background color for clipboard indicator

### `[theme.underline]`

Underline colors (if enabled in `[display.preview_underline]`).

- `fg` - Foreground color
- `bg` - Background color

### `[theme.path]`

Path bar at the top colors.

- `fg` - Foreground color
- `bg` - Background color

### `[theme.archive]`

Tree of an [archive preview](display.md#archive). Names keep the colors of the directory preview.

- `guide` - Color of the tree lines. Follows `[theme.accent]` unless set.
- `modifier` - Text style for names: `"none"` (default), `"dim"`, `"italic"` or `"bold"`. `"italic"` needs a font with an italic style.

```toml
[theme.archive]
guide = "#555555"
modifier = "dim"
```

## Widget Configuration

### `[theme.widget]`

Full widget/dialog theming: position, size, and colors. Leave blank or omit to use regular defaults.

#### `position`

- **Type**: `string or array`
- **Default**: `"center"`

Widget position. Choose one of:
- Preset string: `"center"`, `"top_left"`, `"bottom_right"`, etc. Also possible to write `"topleft"`, `"bottomright"`, etc.
- List: `[x, y]` - percent of screen, e.g., `[38, 32]`
- Table/object: `{ x = 25, y = 60 }` - percent of screen

#### `size`

- **Type**: `string or array or table`
- **Default**: `"small"`

Widget size. Choose one of:
- Preset string: `"small"`, `"medium"`, `"large"`
- List: `[width, height]` - **cells (columns x rows)**, e.g., `[60, 12]`
- Table/object: `{ w = 60, h = 12 }` - **cells**

**Note**: size arrays/tables are now always **cell-based** (not percent-based!)

#### `confirm_size`

- **Type**: `string or array or table`
- **Default**: `"large"`

Confirmation dialog size (for confirmations like deleting files). Same format options as `size`.

#### `move_size`

- **Type**: `array or table`
- **Default**: `[70, 14]`

Move file widget size. Same format options as `size`.

#### `find_visible_results`

- **Type**: `integer`
- **Default**: `8`

Option to specify the maximal `drawn` results of the find widget. Not to be confused with `max_find_results` which calculates the overall maximal results fd will generate.

#### `find_width`

- **Type**: `integer`
- **Default**: `60`

Option to configure the find widget width.

#### Widget Text Colors

- `color.fg` - Styles the overall widget text (foreground)
- `color.bg` - Styles the overall widget text (background)
- `border.fg` - Widget border foreground color
- `border.bg` - Widget border background color
- `title.fg` - Widget title foreground color
- `title.bg` - Widget title background color

#### Widget Labels and Values

- `label.fg` - Change the label color of a widget. Styles the "key" text (e.g. Name:, Size:, Perms:, etc.). Applies to widgets: File Info, Keybind Help.
- `label.bg` - Label background color
- `value.fg` - Change the value color of a widget. Styles the content of the widget data (e.g. main.rs, 4 KB, etc.). Applies to widgets: File Info, Keybind Help.
- `value.bg` - Value background color

#### Go-to Help Widget

- `go_to_help.size` - Set the size of the go to help widget when pressing "g" (default `[58, 3]`)
- `go_to_help.position` - Set the position of the go to help widget (default `"bottom"`)

### `[theme.status_line]`

Configuration for the status line colors.

- `fg` - Foreground color
- `bg` - Background color

**Note**: For marker and copy status, the configuration from `[theme.marker]` sets each status line section.

### `[theme.tab]`

Tab line customization.

- `marker` - Marker icon for active tab
- `separator` - Separator between tabs
- `active.fg` - Active tab foreground color
- `active.bg` - Active tab background color
- `inactive.fg` - Inactive tab foreground color
- `inactive.bg` - Inactive tab background color

#### `line_format`

- **Type**: `string`
- **Default**: `"[{idx}{marker}]"`

Customization of the tab line. Possible placeholders: `{idx}`, `{name}`, `{marker}`, `{separator}`

**Note**: `{name}` shows the current directory of the tab

**Example**: `line_format = "[{idx}{name}{marker}]"`

### `[theme.info]`

Customization for the status line File information colors.

#### Permissions

- `perms.fg` - Permissions text foreground color
- `perms.bg` - Permissions text background color

#### File Size

- `size.fg` - Size text foreground color
- `size.bg` - Size text background color

#### Date/Time Fields

- `date.fg` - Global color for all date/time fields (acts as default) - foreground
- `date.bg` - Global color for all date/time fields (acts as default) - background

**Specific overrides** (take precedence over `date` if defined):
- `modified.fg` / `modified.bg` - Modified timestamp colors
- `created.fg` / `created.bg` - Created timestamp colors
- `accessed.fg` / `accessed.bg` - Accessed timestamp colors

#### File Type

- `file_type.fg` - File type foreground color
- `file_type.bg` - File type background color

#### Owner and Group (Unix/Linux only)

- `owner.fg` - Owner name foreground color
- `owner.bg` - Owner name background color
- `group.fg` - Group name foreground color
- `group.bg` - Group name background color

