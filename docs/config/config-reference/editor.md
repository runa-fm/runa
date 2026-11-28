# Editor Configuration

Editor program selection and file association settings.

## Overview

The `[editor]` section controls which editor opens files and allows per-extension and per-filename overrides.

## Main Options

### `default`

- **Type**: `string`
- **Default**: `"nvim"`

Command to open files (e.g., "nvim", "code", "vim", etc.). This is the default editor used when no extension or filename override matches.

## File-Specific Overrides

### `ext`

- **Type**: `table or inline table`

Map file extensions to editors/program commands. Use single command or array of command + arguments.

Keys can span several extensions, like `"tar.gz"`. The longest matching extension wins.

**Inline format**:
```toml
ext = { rs = "nvim", md = "code", txt = ["code", "-d"], "tar.gz" = ["tar", "-xf"] }
```

**Section format**:
```toml
[editor.ext]
rs = ["vim"]
md = ["code"]
```

### `filename`

- **Type**: `table or inline table`

Map specific filenames to editors/program commands. Use single command or array of command + arguments.

**Inline format**:
```toml
filename = { "Cargo.toml" = "nvim" }
```

**Section format**:
```toml
[editor.filename]
"Cargo.toml" = ["vim"]
```

### `system`

- **Type**: `array of strings`
- **Default**: `[]`

File extensions that open with the default app of the system: `explorer` on Windows, `open` on macOS and `xdg-open` on Linux. They open in the background, runa keeps running.

```toml
[editor]
system = ["pdf", "png", "jpg", "html"]
```

Use this instead of `cmd /C start` or `sh -c "xdg-open ..."`. A shell can read parts of a file name as commands.

## File Variables

Commands can use these variables in their arguments:

| Variable | Value for `/home/a/dl/release.tar.gz` |
|----------|---------------------------------------|
| `{path}` | `/home/a/dl/release.tar.gz` |
| `{name}` | `release.tar.gz` |
| `{dir}`  | `/home/a/dl` |
| `{stem}` | `release` (the name without the matched extension) |
| `{unused_stem}` | `release`, or `release_1`, `release_2`, ... when that name is taken |

Without any variable, the path is appended as the last argument. With variables, nothing is appended.

```toml
[editor.ext]
"tar.gz" = ["tar", "-xf", "{path}", "-C", "{dir}"]
zip = ["7z", "x", "{path}", "-o{unused_stem}"]
```

To use a shell, pass the path as its own argument and read it as `$1`. Never put a file variable inside the command string, a file name could contain shell syntax.

```toml
"tar.gz" = ["sh", "-c", "tar -xf \"$1\" && echo done", "sh", "{path}"]
```

## Running Without the Terminal

By default a command takes over the terminal, like an editor. Use the table form with `terminal = false` to run it in the background. runa keeps running and shows a message if the command fails.

You can also capture command output in the UI using `output = "overlay"` (scrollable ANSI modal) or `output = "message"` (timed notification toast). See [`output`](commands.md#output) for details.

```toml
[editor.ext]
zip = { cmd = ["7z", "x", "-y", "{path}", "-o{unused_stem}"], terminal = false }
pdf = { cmd = ["zathura", "{path}"], terminal = false }
```

Background commands get no input, so they must not ask questions. Use flags like `-y`. Keep interactive tools like `nvim`, `less` or `lazygit` in the default form.

## Extracting Safely

Extract into `{unused_stem}`, so an archive never overwrites an earlier folder of the same name. `7z` creates the folder itself:

```toml
zip = { cmd = ["7z", "x", "{path}", "-o{unused_stem}"], terminal = false }
```

`tar -x` only extracts into a folder that exists. GNU `tar` creates a new one with `--one-top-level`:

```toml
"tar.gz" = { cmd = ["tar", "-xzf", "{path}", "--one-top-level={unused_stem}"], terminal = false }
```

The `tar` of Windows has no such option. Use `-k` there, it never overwrites existing files:

```toml
"tar.gz" = { cmd = ["tar", "-xkzf", "{path}", "-C", "{dir}"], terminal = false }
```

## Editor Resolution

runa resolves the editor to use in the following order:

1. Check if there's a filename override for the exact filename
2. Check the extensions from longest to shortest, first `ext`, then `system`
3. Use the default editor

The resolved editor can be a single command string or an array of command + arguments. If you set a custom filename to a certain program/cmd, the actual file path will be handled in the actual file path directory and never in the starting directory of runa.

