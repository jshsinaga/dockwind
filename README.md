# Dockwind

A floating dock bar for Windows 10 and 11, written in Rust. Dockwind hides the
native taskbar instead of removing it, and draws its own dock with configurable
transparency, corner radius, and colors.

Dockwind shows pinned and running apps, a clock and date, and optional auto-hide.
It supports a native right-click menu, per-user autostart, a global quit hotkey,
and a standalone taskbar restore command.

Current build status: the final full gate passed on 2026-11-10. Manual desktop
cases remain unverified. See
[docs/HANDOFF.md](docs/HANDOFF.md) for dated evidence and the checks that are
still open.

## Documentation

| File | Audience |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Product scope, feature status, acceptance criteria |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Module ownership and lifetimes |
| [DESIGN.md](DESIGN.md) | UI and window patterns |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Every config field, validation, and reload behavior |
| [docs/HANDOFF.md](docs/HANDOFF.md) | Current state, pending work, and evidence |
| [docs/decisions.md](docs/decisions.md) | Accepted decisions and trade-offs |

## Requirements

- Windows 10 or 11, x64.
- Rust 1.99.1 with the MSVC target. `rust-toolchain.toml` pins the toolchain,
  `rustfmt`, `clippy`, and `x86_64-pc-windows-msvc`.
- Visual Studio 2022 Build Tools with the C++ workload and a Windows SDK.
  The current build uses MSVC 14.44 and Windows SDK 10.0.26100.

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

## Restoring the Taskbar

Dockwind only hides `Shell_TrayWnd` and `Shell_SecondaryTrayWnd`. It does not stop
`explorer.exe`. The taskbar is restored on normal quit, panic, console control
events, and session end. If Dockwind is killed, run:

```powershell
dockwind --restore-taskbar
```

The default quit hotkey is `Ctrl+Alt+Shift+Q`. Dockwind uses a named mutex to
prevent multiple instances. `--restore-taskbar` runs even when another instance
is active.

## Autostart

Autostart uses `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Right-click
the dock and choose Enable autostart or Disable autostart. The same operations are
available from the command line:

```powershell
dockwind --enable-autostart
dockwind --disable-autostart
```

## Configuration

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

Read `docs/CONFIGURATION.md` for field types, defaults, validation, and which
changes need a restart.

## Logs

Logs are written daily under:

```text
%LOCALAPPDATA%\Dockwind\logs\dockwind.log.YYYY-MM-DD
```

Set `RUST_LOG` to change the filter, for example `RUST_LOG=dockwind=debug`.
Logs also go to stdout.

## Windows Manifest

GPUI's default manifest is disabled so Dockwind can own a single `RT_MANIFEST`
resource. `assets/dockwind.manifest` sets:

- Per-monitor v2 DPI awareness.
- `longPathAware`.
- Common Controls v6.
- The Dockwind assembly identity.

The embedded manifest is verified with `mt.exe` after building.

## Known Limitations

- The dock content is fixed at 560x92 logical pixels. Large app lists do not
  scroll yet.
- `reserve_space` is accepted in config but has no effect. AppBar reservation is
  not implemented.
- `clock.locale` is accepted but unused. Clock text uses the process locale.
- `hide_native_taskbar`, `quit_hotkey`, and `margin_bottom` are read at startup.
  Changing them by hot reload does not apply until the next launch.
- Placement is computed once at startup. A display change, DPI change, or taskbar
  work-area change needs a restart.
- The Mica and Acrylic backdrop branches need Windows 11 22H2 or newer and have
  not been verified. The current machine is Windows 10 22H2. On Windows 10 the
  dock logs that the backdrop is unavailable and stays transparent.
- Monitor placement is verified in the running UI at 100% DPI. The physical to
  logical conversion and dock rectangle are unit-tested at 100%, 125%, 150%, and
  200% DPI. A real 125% to 200% desktop was not available for manual testing.
- Auto-hide fullscreen suppression is unit-tested, but a real fullscreen game or
  video was not exercised on this machine.
- The native taskbar hide/restore path is tested for normal quit, forced process
  kill plus `--restore-taskbar`, and a simulated `TaskbarCreated` broadcast. A
  real `explorer.exe` restart was not performed.
- UWP windows hosted by `ApplicationFrameHost.exe` are skipped because their real
  app identity is not mapped yet.
- Middle-click to launch a second instance, tooltips, and scrolling for large app
  lists are not implemented yet.
- There is no settings window and no installer. The settings and installer
  requirements are future work in `docs/PRD.md`.
- GPUI is pinned to `=0.2.2` because it is pre-1.0 and its API changes between
  releases.
