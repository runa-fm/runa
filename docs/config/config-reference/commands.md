# Commands

Own commands bound to keys.

## Overview

The `[commands]` section binds programs to keys. Each command has a name, the keys that run it and the command itself. Commands start on demand and run like editor commands, in the terminal or in the background.

```toml
[commands]
lazygit = { keys = "<m-g>", cmd = "lazygit" }
extract = { keys = "x", cmd = ["7z", "x", "-o{unused_stem}", "{path}"], terminal = false }
status  = { keys = "<m-s>", cmd = ["git", "status"], output = "overlay" }
push    = { keys = "<m-p>", cmd = ["git", "push"], output = "message" }
```

## Options

### `keys`

- **Type**: `string or array of strings`

Keys that run the command. Uses the same syntax as [Key Bindings](keys.md). A command key replaces a built-in action on the same key.

The prefix keys (`g` and `o` by default) always start go-to and sort, so a command on one of them never runs.

### `cmd`

- **Type**: `string or array of strings`

The program and its arguments. Unlike editor commands, no path is appended.

[File variables](editor.md#file-variables) refer to the selected entry. A command that uses them needs a selected entry. `{stem}` and `{unused_stem}` drop only the last extension.

### `terminal`

- **Type**: `boolean`
- **Default**: `true` (when `output` is not set)

With `true`, the command takes over the terminal, like an editor. With `false`, it runs in the background and runa shows a message if it fails. See [Running Without the Terminal](editor.md#running-without-the-terminal).

Cannot be set to `true` together with `output`.

### `output`

- **Type**: `string`
- **Options**: `"overlay"`, `"message"`

Runs the command in the background and shows its stdout and stderr inside runa with ANSI color support:

- `"overlay"` — Opens a scrollable modal overlay showing the command's output. Scroll with `j`/`k` or `Up`/`Down`, `PgUp`/`PgDn`, `g`/`G` or `Home`/`End`, pan horizontally with `h`/`l` or `Left`/`Right` (`0` resets horizontal scroll), and close with `q` or `Esc`.
- `"message"` — Shows up to 12 lines of output in a timed notification message.

Setting `output` implies running in the background (`terminal = false`).

## Working Directory

Commands run in the directory runa shows.
