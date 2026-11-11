# Dockwind Configuration

Status: active. This document owns the config schema, validation, reload behavior,
and the settings save contract. It does not own product scope or architecture.

Last updated: 2026-11-11
Code: settings window, tray, and live geometry integrated. Full gate evidence is
in `docs/HANDOFF.md`.

## 1. Location and Lifecycle

| Item | Value |
|---|---|
| Config path | `%APPDATA%\Dockwind\config.toml` |
| Log path | `%LOCALAPPDATA%\Dockwind\logs\dockwind.log.YYYY-MM-DD` |
| Auto-hide marker | `%LOCALAPPDATA%\Dockwind\taskbar-auto-hide` |
| Created | On first run, from defaults |
| Encoding | UTF-8 TOML |
| Hot reload | `notify` watcher with a 200 ms debounce |
| Settings entry | Dock context menu, tray menu, or `dockwind --settings` |
| Settings instances | One window per user session through `Local\DockwindSettings`; a second launch focuses the existing window |

If the file is missing, Dockwind writes a default file with every field. If the
file is corrupt at startup, Dockwind logs a warning and uses defaults. If the file
is corrupt during hot reload, Dockwind logs an error and keeps the last valid
config. Dockwind does not overwrite a corrupt user file unless the user saves from
the settings window and confirms the overwrite.

## 2. Schema

```toml
[appearance]
background = "#1E1E2ECC"
corner_radius = 16
blur = "none"
icon_size = 40
padding = 8
width = 560
height = 92
offset = 12
orientation = "bottom"
opacity = 100

[behavior]
auto_hide = true
show_delay_ms = 80
hide_delay_ms = 400
animation_ms = 180
reserve_space = false
hide_native_taskbar = false
hide_in_fullscreen = true
show_running_apps = true
quit_hotkey = "Ctrl+Alt+Shift+Q"

[clock]
time_format = "%H:%M"
date_format = "%a, %d %b %Y"
locale = "en"

[[pinned]]
name = "Terminal"
path = 'C:\Windows\System32\cmd.exe'
args = []
```

All sections and fields are optional. Missing values use defaults. Unknown keys
are ignored on load. A duplicate pinned path is not rejected and produces a
duplicate item.

`margin_bottom` is accepted as a legacy alias for `offset`. New files use
`offset`.

## 3. Field Reference

### 3.1 `[appearance]`

| Field | Type | Default | Validation | Reload |
|---|---|---|---|---|
| `background` | string | `#1E1E2ECC` | `#RRGGBB` or `#RRGGBBAA` | Live |
| `corner_radius` | integer | 16 | Capped at half of `icon_size + 2 * padding` | Live |
| `blur` | enum | `none` | `none`, `mica`, or `acrylic` | Live |
| `icon_size` | integer | 40 | 16 to 128, and no more than `height - 16` | Live |
| `padding` | integer | 8 | Fits in `height` with `icon_size`; checked arithmetic | Live |
| `width` | integer | 560 | 200 to 4096 | Live |
| `height` | integer | 92 | 48 to 400 | Live |
| `offset` | integer | 12 | 0 to 512. Alias `margin_bottom` | Live |
| `orientation` | enum | `bottom` | `bottom` or `top` | Live |
| `opacity` | integer | 100 | 10 to 100 | Live |

`background` alpha is written in the color. `opacity` is a percentage multiplier
on that alpha. At `opacity = 100`, the background is exactly the written value. At
`opacity = 50`, a `#1E1E2ECC` background becomes `#1E1E2E66`.

`width`, `height`, and `offset` are live. Changing them from the settings window
or by editing the file repositions and resizes the dock without a restart.

`orientation = "top"` places the dock at the top of the work area and moves the
auto-hide trigger to the top edge. `orientation = "bottom"` is the default.

`padding` uses checked arithmetic. If `padding * 2 + icon_size` overflows `u32`,
Dockwind replaces `padding` with 8 and logs a warning. It is then capped so the
content fits inside `height`.

`corner_radius` is replaced with `min(default, max_radius)` when it exceeds the
cap. The cap is half of `icon_size + 2 * padding`.

`blur = "mica"` and `blur = "acrylic"` need Windows 11 build 22621 or newer. On an
older build, Dockwind logs that the backdrop is unavailable and keeps the
transparent surface. This is an explicit, logged degradation.

### 3.2 `[behavior]`

| Field | Type | Default | Validation | Reload |
|---|---|---|---|---|
| `auto_hide` | bool | true | None | Live |
| `show_delay_ms` | integer | 80 | None | Live |
| `hide_delay_ms` | integer | 400 | None | Live |
| `animation_ms` | integer | 180 | None. Zero means an instant step | Live |
| `reserve_space` | bool | false | None | Live |
| `hide_native_taskbar` | bool | false | None | Live |
| `hide_in_fullscreen` | bool | true | None | Live |
| `show_running_apps` | bool | true | None | Live |
| `quit_hotkey` | string | `Ctrl+Alt+Shift+Q` | Parsed at startup and by the settings window | Startup only |

`auto_hide = false` keeps the dock permanently visible. The settings window shows
this as the unchecked "Auto-hide when not used" box.

`show_running_apps = false` hides running apps that are not pinned. Pinned apps
still show whether they are running.

`reserve_space = true` registers an AppBar reservation for the dock strip. It
positions the reservation to match the dock edge and dimensions. The reservation
is created and removed live. The reserved rect is removed on shutdown and on
disabling the field. A forced process kill can leave the work area reserved until
`--restore-taskbar` runs.

`hide_native_taskbar` is applied live. Enabling it installs the panic hook,
console handler, and taskbar watcher before hiding. Disabling it restores the
taskbar before stopping the watcher. The runtime records whether the native
taskbar used auto-hide, disables it while the taskbar is hidden, and restores it
on disable, normal quit, and panic/console recovery. A replacement taskbar is
re-hidden by a two-second watcher check, and an explicitly restored taskbar is not
re-hidden. A forced kill leaves the state in
`%LOCALAPPDATA%\Dockwind\taskbar-auto-hide`; `--restore-taskbar` reads it,
restores auto-hide, and removes it. When it is true at startup,
an invalid or unavailable `quit_hotkey` fails startup because the user would lose
the safe quit path.

`quit_hotkey` is read at startup. Changing it saves the value, validates it, and
shows a restart notice.

### 3.3 `[clock]`

| Field | Type | Default | Validation | Reload |
|---|---|---|---|---|
| `time_format` | string | `%H:%M` | Valid `chrono` strftime pattern | Live |
| `date_format` | string | `%a, %d %b %Y` | Valid `chrono` strftime pattern | Live |
| `locale` | string | `en` | None | No effect |

Invalid time or date formats are replaced with defaults and logged. The clock
uses the process locale. `locale` is accepted for compatibility but is unused.

### 3.4 `[[pinned]]`

| Field | Type | Required | Notes |
|---|---|---|---|
| `name` | string | Yes | Display name |
| `path` | string | Yes | Executable or launchable path. Matching is case-insensitive |
| `args` | array of strings | No, defaults to empty | Quoted and passed to `ShellExecuteW` |

Pinned apps keep their file order. Running windows from the same executable are
merged into the pinned item. A missing `name` or `path` field is a parse failure.
An empty path string is not validated at load time. The launch action reports the
error when clicked.

The settings window can add, remove, and reorder pinned apps. It uses the file
name as the display name and writes an empty argument list.

## 4. Validation Rules

1. `width` must be in `200..=4096`. Otherwise it becomes 560 with a warning.
2. `height` must be in `48..=400`. Otherwise it becomes 92 with a warning.
3. `icon_size` must be in `16..=128`. Otherwise it becomes 40 with a warning. It
   is then capped at `height - 16`.
4. `padding` must allow `padding * 2 + icon_size` to fit in `u32`. Otherwise it
   becomes 8 with a warning. It is then capped so the content fits in `height`.
5. `opacity` must be in `10..=100`. Otherwise it becomes 100 with a warning.
6. `offset` must be at most 512. Otherwise it becomes 12 with a warning.
7. `corner_radius` must not exceed `(icon_size + 2 * padding) / 2`. Otherwise it
   becomes the smaller of the default and the cap, with a warning.
8. `background` must be `#RRGGBB` or `#RRGGBBAA`. Otherwise it becomes the default,
   with a warning.
9. `time_format` and `date_format` must contain no `chrono` format errors.
   Otherwise each becomes its default, with a warning.
10. `quit_hotkey` must parse at startup. A bad value is a startup error. If the
    taskbar is not hidden, Dockwind logs a warning and runs without a global quit
    hotkey. The settings window rejects an invalid hotkey with a message.
11. A TOML type error is a parse failure. Startup uses defaults with a warning.
    Reload keeps the last valid config with an error.
12. Unknown keys are ignored. They are not preserved by a rewrite.

Validation never panics. The application must not crash because of a bad config
value.

## 5. Reload Behavior

The watcher resolves the config path with `std::path::absolute` without
canonicalization, then watches its parent directory, not the file handle. This
keeps relative paths matching the paths reported by `notify`, lets a missing file
be created later, and keeps reloads working when an editor or a save path replaces
the file atomically instead of editing it in place. Events for other files in the
directory are ignored.

The watcher drains matching events for 200 ms. A matching event extends the
deadline. It then parses the file and sends a full `Config` to the dock.
`ConfigWatcher::shutdown` is idempotent, stops the worker, and joins it. A failed
path resolution returns `ConfigError::Resolve`.

The following changes apply live:

- `appearance.background`
- `appearance.corner_radius`
- `appearance.blur`
- `appearance.icon_size`
- `appearance.padding`
- `appearance.width`
- `appearance.height`
- `appearance.offset`
- `appearance.orientation`
- `appearance.opacity`
- `clock.time_format`
- `clock.date_format`
- `behavior.auto_hide`
- `behavior.show_delay_ms`
- `behavior.hide_delay_ms`
- `behavior.animation_ms`
- `behavior.hide_in_fullscreen`
- `behavior.show_running_apps`
- `behavior.hide_native_taskbar`
- `behavior.reserve_space` (requires hiding the taskbar)
- `[[pinned]]`

The following changes need a restart or have no effect:

- `behavior.quit_hotkey` (restart)
- `clock.locale` (no effect)

A config reload does not rewrite the file. The user's text, ordering, comments,
and unknown keys stay on disk until the user or the settings window writes it.

## 6. Settings Save Behavior

The settings window is a native Win32 window on its own thread. It is not the
GPUI dock window. The reason is recorded in `docs/decisions.md`.

### 6.1 One Writer

- The settings window is single-instance within a process.
- The dock does not write the config file.
- The watcher reads a settings save like any other external edit.
- `dockwind --settings` opens a separate process when the dock is running. The
  running dock reloads the file through the watcher.

### 6.2 Apply and Close

- Controls hold the draft.
- Apply validates, saves, and updates the controls with sanitized values.
- Close discards unsaved edits and closes the window.
- Validation failures show a message box and do not write the file.
- Autostart is applied immediately. It is not a config field.

### 6.3 Atomic Replacement

1. Serialize the complete config to a temporary file in the same directory.
2. Flush the temporary file.
3. Replace the target file in one rename.
4. Remove the temporary file on failure.
5. Keep the previous file if a step fails.

Do not write in place. A crash during an in-place write can leave a corrupt file.

### 6.4 External Edits and Conflicts

The settings window records the file contents when it opens. Before Apply it reads
the file again. If the contents changed, it asks before overwriting. If the file
was deleted, it asks before recreating it. A failed save leaves the existing file
intact and reports the error.

The writer does not merge external field values into the draft. Overwrite applies
the draft's known fields to the current valid document after confirmation. An
invalid current file is not overwritten.

### 6.5 Preserved User Content

A valid file keeps its comments, key ordering, and unknown keys. The writer
updates known keys in place through `toml_edit`; it does not serialize a fresh
document. The `pinned` array is rewritten from the draft, so comments and unknown
keys inside `pinned` entries are not preserved. A file with invalid TOML is
rejected by Apply and left unchanged.

### 6.6 Restart Notices

The settings window shows a restart notice after saving a change to:

- `behavior.quit_hotkey`

`behavior.hide_native_taskbar` is live, so it does not need a restart.
`behavior.reserve_space` is live when the taskbar is hidden. The settings window
rejects it while the taskbar is visible. `clock.locale` has no control.
`appearance.offset` is live, so it does not need a restart.

## 7. Compatibility Rules

1. Existing keys keep their names, types, and meaning. `margin_bottom` remains
   accepted as an alias for `offset`.
2. Missing keys use defaults. A partial file remains valid.
3. Unknown keys are ignored on load. The writer does not preserve them.
4. `reserve_space` is accepted and effective when `hide_native_taskbar` is true.
   `clock.locale` remains accepted even though it has no effect.
5. The config path and log path stay stable.
6. The CLI supports `--restore-taskbar`, `--enable-autostart`,
   `--disable-autostart`, and `--settings`. The existing commands keep their
   behavior.
7. A config change must not change the taskbar restore contract or the startup
   order in `docs/ARCHITECTURE.md`.

## 8. Failure Messages

Use these shapes when reporting config problems:

| Case | Message intent |
|---|---|
| Invalid value | Name the field, the bad value, and the replacement |
| Corrupt startup file | State that defaults are active until the file is fixed |
| Corrupt reload | State that the last valid config is still active |
| Failed save | State that the existing file is unchanged |
| External edit | State that the file changed and ask before overwriting |
| Path resolution failed | Report `ConfigError::Resolve` with the path and source |
| Watcher stopped | Report `ConfigError::WatcherPanicked` if the worker panicked |
| Restart-only field | Name the field and state that a restart is required |

Do not swallow a write or parse error. Log it or show it to the user.

## 9. Settings and Tray Entry Points

| Entry | Action |
|---|---|
| Dock right-click | Settings..., Reload config, autostart toggle, Restore taskbar & Quit |
| Tray right-click | Settings..., Reload config, Start with Windows, Restore taskbar, Quit |
| Tray double-click | Open settings |
| `dockwind --settings` | Open the settings window and wait for it to close |

The tray icon is created by a dedicated worker thread. It is removed on normal
shutdown and re-added if Explorer broadcasts `TaskbarCreated`. While the native
taskbar is visible, a two-second check also re-adds a missing icon. A missing tray
icon is logged and does not block the dock.
