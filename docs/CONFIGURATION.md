# Dockwind Configuration

Status: active. This document owns the config schema, validation, reload behavior,
and the future save contract. It does not own product scope or architecture.

Last updated: 2026-11-10
Code: integrated, review-cleared, and the final full gate passed on 2026-11-10.
Historical baseline: `482bd7a`

## 1. Location and Lifecycle

| Item | Value |
|---|---|
| Config path | `%APPDATA%\Dockwind\config.toml` |
| Log path | `%LOCALAPPDATA%\Dockwind\logs\dockwind.log.YYYY-MM-DD` |
| Created | On first run, from defaults |
| Encoding | UTF-8 TOML |
| Hot reload | `notify` watcher with a 200 ms debounce |

If the file is missing, Dockwind writes a default file with every field. If the
file is corrupt at startup, Dockwind logs a warning and uses defaults. If the file
is corrupt during hot reload, Dockwind logs an error and keeps the last valid
config. Dockwind does not overwrite a corrupt user file.

## 2. Schema

```toml
[appearance]
background = "#1E1E2ECC"
corner_radius = 16
blur = "none"
icon_size = 40
padding = 8
margin_bottom = 12

[behavior]
auto_hide = true
show_delay_ms = 80
hide_delay_ms = 400
animation_ms = 180
reserve_space = false
hide_native_taskbar = false
hide_in_fullscreen = true
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

## 3. Field Reference

### 3.1 `[appearance]`

| Field | Type | Default | Validation | Reload |
|---|---|---|---|---|
| `background` | string | `#1E1E2ECC` | `#RRGGBB` or `#RRGGBBAA` | Live |
| `corner_radius` | integer | 16 | Capped at half of `icon_size + 2 * padding` | Live |
| `blur` | enum | `none` | `none`, `mica`, or `acrylic` | Live |
| `icon_size` | integer | 40 | 16 to 128 | Live |
| `padding` | integer | 8 | Only a value that makes `padding * 2 + icon_size` unrepresentable in `u32` is replaced | Live |
| `margin_bottom` | integer | 12 | None | Startup only |

`padding` uses checked arithmetic. If `padding * 2 + icon_size` overflows `u32`,
Dockwind replaces `padding` with 8 and logs a warning. There is no lower or upper
limit for a representable padding value.

`corner_radius` is replaced with `min(default, max_radius)` when it exceeds the
cap. The cap is half of `icon_size + 2 * padding`, not half of the full window
height.

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
| `reserve_space` | bool | false | None | No effect |
| `hide_native_taskbar` | bool | false | None | Startup only |
| `hide_in_fullscreen` | bool | true | None | Live |
| `quit_hotkey` | string | `Ctrl+Alt+Shift+Q` | Parsed at startup | Startup only |

`reserve_space` is accepted for compatibility but is not implemented. No AppBar
registration, space reservation, or `ABM_REMOVE` exists. The field has no runtime
effect.

`hide_native_taskbar` is read at startup. Changing it by hot reload does not apply
until the next launch. When it is true, an invalid or unavailable `quit_hotkey`
fails startup because the user would lose the safe quit path.

`margin_bottom` is used when the window is created. It is not part of the live
appearance update, so changing it needs a restart.

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

## 4. Validation Rules

1. `icon_size` must be in `16..=128`. Otherwise it becomes 40 with a warning.
2. `padding` must allow `padding * 2 + icon_size` to fit in `u32`. Otherwise it
   becomes 8 with a warning. Representable values are accepted without an
   arbitrary limit.
3. `corner_radius` must not exceed `(icon_size + 2 * padding) / 2`. Otherwise it
   becomes the smaller of the default and the cap, with a warning.
4. `background` must be `#RRGGBB` or `#RRGGBBAA`. Otherwise it becomes the default,
   with a warning.
5. `time_format` and `date_format` must contain no `chrono` format errors.
   Otherwise each becomes its default, with a warning.
6. `quit_hotkey` must parse at startup. A bad value is a startup error. If the
   taskbar is not hidden, Dockwind logs a warning and runs without a global quit
   hotkey.
7. A TOML type error is a parse failure. Startup uses defaults with a warning.
   Reload keeps the last valid config with an error.
8. Unknown keys are ignored. They are not preserved by a rewrite.

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
path resolution returns `ConfigError::Resolve`. This section states the reload
contract. Read `docs/HANDOFF.md` for current verification status. The following
changes apply live:

- `appearance.background`
- `appearance.corner_radius`
- `appearance.blur`
- `appearance.icon_size`
- `appearance.padding`
- `clock.time_format`
- `clock.date_format`
- `behavior.auto_hide`
- `behavior.show_delay_ms`
- `behavior.hide_delay_ms`
- `behavior.animation_ms`
- `behavior.hide_in_fullscreen`
- `[[pinned]]`

The following changes need a restart or have no effect:

- `appearance.margin_bottom`
- `behavior.hide_native_taskbar`
- `behavior.quit_hotkey`
- `behavior.reserve_space` (no effect)
- `clock.locale` (no effect)

A config reload does not rewrite the file. The user's text, ordering, comments,
and unknown keys stay on disk until the user or a future settings UI writes it.

## 6. Save Contract for the Future Settings UI

This contract is required before a settings UI ships. It is not implemented in the
current release.

### 6.1 One Writer

- One shared config owner serializes all writes.
- The settings window and the dock do not write the file concurrently.
- The owner validates the draft before writing.
- A failed write leaves the previous file intact.

### 6.2 Atomic Replacement

1. Serialize the complete config to a temporary file in the same directory.
2. Flush the temporary file.
3. Replace the target file in one rename or replace operation.
4. Keep the previous file if any step fails.
5. Report the failure. Do not fall back to a partial write.

Do not write in place. A crash during an in-place write can leave a corrupt file.

### 6.3 Watcher Feedback

- A settings save and the file watcher must not fight each other.
- The owner records the content hash or revision it wrote.
- The watcher ignores its own save, or reports it as a no-op.
- The dock updates from the owner's validated config, not from a second parse of
  the same write.
- An external save still reaches the dock through the normal watcher path.

### 6.4 External Edits and Conflicts

- Capture the file revision or content hash when the settings draft opens.
- Before Apply, check the current file revision.
- If the file changed, do not overwrite it silently. Offer to reload or to
  overwrite, and state which changes will be lost.
- If the file was deleted, offer to recreate it from the draft or cancel.
- If the file is corrupt, keep the draft and show the parse error.

### 6.5 Preserving User Content

- Keep existing keys, section order where practical, comments, and unknown keys.
- Do not drop a key only because the current build does not use it.
- Do not rename keys or change defaults as part of a settings save.
- Preserve `[[pinned]]` order.
- If the format cannot preserve a comment or unknown key, warn before writing.

The current loader ignores unknown keys and the current serializer would drop
them. That is a known limitation of the current code, not the future contract.

### 6.6 Restart Notices

The settings UI must show a restart-required notice for:

- `behavior.hide_native_taskbar`
- `behavior.quit_hotkey`
- `appearance.margin_bottom`

`behavior.reserve_space` and `clock.locale` must be shown as unavailable, not as
working settings.

## 7. Compatibility Rules

1. Existing keys keep their names, types, and meaning. Do not rename or remove a
   key without a decision that records the migration.
2. Missing keys use defaults. A partial file remains valid.
3. Unknown keys are ignored on load. A future writer should preserve them where
   possible.
4. `reserve_space` and `clock.locale` remain accepted even though they have no
   effect. Removing them would break existing files.
5. The config path and log path stay stable.
6. The CLI stays `--restore-taskbar`, `--enable-autostart`, and
   `--disable-autostart`. The config work does not add flags.
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
| Path resolution failed | Report `ConfigError::Resolve` with the path and source |
| Watcher stopped | Report `ConfigError::WatcherPanicked` if the worker panicked |
| Restart-only field | Name the field and state that a restart is required |

Do not swallow a write or parse error. Log it or show it to the user.
