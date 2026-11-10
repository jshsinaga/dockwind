# Dockwind

A floating dock bar for Windows 10 and 11, written in Rust. Dockwind hides the
native taskbar instead of removing it, and draws its own dock with configurable
size, offset, orientation, transparency, corner radius, and color.

Dockwind shows pinned and running apps, a clock and date, and optional auto-hide.
It has a native settings window, a system tray icon, per-user autostart, a global
quit hotkey, a standalone taskbar restore command, and an Inno Setup installer.

Current build status: the full gate passes on the current source. Settings, tray,
live geometry, live taskbar toggling, native auto-hide suspension, AppBar
reservation, startup hide, normal quit restore, Explorer restart recovery, the
recovery CLI, the basic installer path, and the release dry run were manually
checked on 2026-11-11. Panic/console recovery, a clean-profile install, and the
tag publish are still open. See [docs/HANDOFF.md](docs/HANDOFF.md) for dated
evidence.

## Documentation

| File | Audience |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Product scope, feature status, acceptance criteria |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Module ownership and lifetimes |
| [DESIGN.md](DESIGN.md) | UI and window patterns |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Every config field, validation, reload, and save behavior |
| [docs/RELEASE.md](docs/RELEASE.md) | Versioning, installer, and GitHub Actions |
| [docs/HANDOFF.md](docs/HANDOFF.md) | Current state, pending work, and evidence |
| [docs/decisions.md](docs/decisions.md) | Accepted decisions and trade-offs |

## Requirements

- Windows 10 or 11, x64.
- Rust 1.99.1 with the MSVC target. `rust-toolchain.toml` pins the toolchain,
  `rustfmt`, `clippy`, and `x86_64-pc-windows-msvc`.
- Visual Studio 2022 Build Tools with the C++ workload and a Windows SDK.
  The current build uses MSVC 14.44 and Windows SDK 10.0.26100.
- Inno Setup 6 only for the local installer build. It is not needed for
  `cargo build`.

## Build and Run

```powershell
cargo build
cargo run
```

Release build:

```powershell
cargo build --release
```

Run the local gate from any directory:

```powershell
.\scripts\check.ps1
```

The script runs format, check, clippy, tests, the release build, and local
Markdown link checks. It stops at the first failed command.

The underlying commands are:

```powershell
cargo fmt --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

## Settings Window

Open it from the dock's right-click menu, the tray menu, a tray double-click, or:

```powershell
dockwind --settings
```

The settings window is a native Win32 window on its own thread. It edits the same
config file as the dock. Apply validates and saves atomically. A valid file keeps
its comments, ordering, and unknown keys; pinned entries are rewritten. Close
discards unsaved edits. If the file changed on disk while the window was open,
Apply asks before overwriting. Only one settings window is open per session;
launching settings again focuses it.

The window covers:

- Appearance: width, height, icon size, padding, corner radius, offset,
  orientation (top or bottom), opacity, background color with a color picker.
- Behavior: auto-hide, hide during fullscreen apps, show running apps, hide native
  taskbar, autostart, quit hotkey.
- Clock: time and date formats.
- Pinned apps: add, remove, and reorder.

`hide_native_taskbar` applies live, including the taskbar watcher and recovery
hooks. While the native taskbar is hidden, Dockwind suspends its auto-hide setting
and restores it on quit. A two-second watcher fallback re-hides a replacement
taskbar after an Explorer restart. `quit_hotkey` needs a restart and the window
shows a notice after saving it. `reserve_space` is available when the native
taskbar is hidden. The settings window rejects the combination without hiding.

## Tray

A Dockwind tray icon appears while the dock is running. Right-click it for:

- Settings...
- Reload config
- Start with Windows
- Restore taskbar
- Quit

Double-click opens settings. The tray icon is re-added after an Explorer restart,
checked every two seconds while the taskbar is visible, and removed on normal
shutdown.

## Theming

The dock reads its appearance from the config file and applies changes live:

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
```

- `background` accepts `#RRGGBB` or `#RRGGBBAA`.
- `opacity` multiplies the background alpha from 10 to 100 percent.
- `orientation` is `bottom` or `top`. The auto-hide trigger follows the edge.
- `width`, `height`, and `offset` reposition or resize the dock without a restart.
- `margin_bottom` is accepted as a legacy alias for `offset`.

Read [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for every field, its range,
and its reload behavior.

## Reserve Screen Space

With `hide_native_taskbar = true` and `reserve_space = true`, Dockwind registers
an AppBar reservation for the dock strip. Maximized windows stop above the dock
instead of covering it. The reservation is removed when the field is disabled and
on normal quit. After a forced kill, run `dockwind --restore-taskbar` to restore
the taskbar and work area. It is not enabled while the native taskbar is visible.

## App Management

- Pinned apps keep their order and are merged with running windows from the same
  executable.
- Running-only apps appear after the pinned list.
- `behavior.show_running_apps = false` hides running-only apps.
- The settings window can add, remove, and reorder pinned apps.
- Left-click activates, restores, minimizes, or launches an app.

## Restoring the Taskbar

Dockwind only hides `Shell_TrayWnd` and `Shell_SecondaryTrayWnd`. It does not stop
`explorer.exe`. The taskbar is restored on normal quit, panic, console control
events, and session end. If Dockwind is killed, run:

```powershell
dockwind --restore-taskbar
```

The default quit hotkey is `Ctrl+Alt+Shift+Q`. Dockwind uses a named mutex to
prevent multiple dock instances. `--restore-taskbar` runs even when another
instance is active. If the native taskbar auto-hide was suspended, the command
restores it from `%LOCALAPPDATA%\Dockwind\taskbar-auto-hide`.

## Autostart

Autostart uses `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Enable it
from the tray, the settings window, or the command line:

```powershell
dockwind --enable-autostart
dockwind --disable-autostart
```

## Release and CI

Versioning, local installer builds, and the GitHub Actions workflows are in
[docs/RELEASE.md](docs/RELEASE.md).

Local installer:

```powershell
.\scripts\build-installer.ps1
```

Output: `dist\Dockwind-<version>-setup.exe`.

The installer is per-user, creates a Start Menu shortcut, offers an optional
desktop shortcut, and does not force autostart. Uninstall runs
`--disable-autostart` and `--restore-taskbar` before removing files. User config
and logs are preserved. A silent install and uninstall passed on 2026-11-11. A
clean Windows profile and upgrade were not tested.

CI runs `scripts/check.ps1` on `windows-latest` for pushes and pull requests. On
push and manual runs, a second job builds the installer and uploads the
`Dockwind-installer` artifact. A `vX.Y.Z` tag builds the installer and publishes a
GitHub Release.

## Configuration File

The config file is created on first run at:

```text
%APPDATA%\Dockwind\config.toml
```

Default values are written for every field, so a partial config is valid.
Invalid values are reported in the log and replaced with defaults. Saving the
file triggers hot reload after a 200 ms debounce. If a reload fails to parse,
Dockwind logs the error and keeps the last valid configuration.

The config sections are `[appearance]`, `[behavior]`, `[clock]`, and `[[pinned]]`.
Example pinned app:

```toml
[[pinned]]
name = "Terminal"
path = 'C:\Windows\System32\cmd.exe'
args = []
```

## Logs

Logs are written daily under:

```text
%LOCALAPPDATA%\Dockwind\logs\dockwind.log.YYYY-MM-DD
```

Set `RUST_LOG` to change the filter, for example `RUST_LOG=dockwind=debug`.
Logs also go to stdout.

## Windows Manifest and Version

GPUI's default manifest is disabled so Dockwind can own a single `RT_MANIFEST`
resource. `assets/dockwind.manifest` sets:

- Per-monitor v2 DPI awareness.
- `longPathAware`.
- Common Controls v6.
- The Dockwind assembly identity.

`build.rs` injects the `Cargo.toml` version into the manifest and embeds the
manifest and icon. The embedded resources are checked by the build.

## Known Limitations

- The app list scrolls horizontally. Tooltips and a visible overflow affordance
  are not implemented.
- `reserve_space` requires `hide_native_taskbar = true`. It reserves the dock
  strip through an AppBar. A forced process kill can leave the work area
  reserved until `--restore-taskbar` runs.
- Dockwind suspends native taskbar auto-hide while the taskbar is hidden. A
  forced kill can leave auto-hide disabled until `--restore-taskbar` runs; that
  command restores the recorded setting from the marker file.
- `clock.locale` is accepted but unused. Clock text uses the process locale.
- `quit_hotkey` is read at startup. The settings window shows a restart notice.
  `hide_native_taskbar` applies live.
- Placement updates live for config changes and is polled once per second for
  work-area, display, and DPI changes. A real multi-DPI or resolution change has
  not been manually exercised.
- The Mica and Acrylic backdrop branches need Windows 11 22H2 or newer and have
  not been verified. The current machine is Windows 10 22H2.
- The running UI was exercised at 100% DPI only. The physical to logical
  conversion and dock rectangle are unit-tested at 100%, 125%, 150%, and 200%.
- Panic restore, console-control restore, and a fullscreen app check are still
  pending. A real Explorer restart passed.
- UWP windows hosted by `ApplicationFrameHost.exe` are skipped.
- App tooltips are not implemented yet. Middle-click launch for pinned apps is
  implemented.
- The installer is unsigned. Windows SmartScreen may warn on first run.
- The GitHub Actions CI gate passed on 2026-11-11. A manual release dry run built
  and uploaded the installer; the tag publish path has not run yet.
- GPUI is pinned to `=0.2.2` because it is pre-1.0 and its API changes between
  releases.
