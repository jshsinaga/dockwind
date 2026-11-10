# Dockwind

A floating dock bar for Windows 10 and 11, written in Rust. Dockwind hides the
native taskbar instead of removing it, and draws its own dock with configurable
transparency, corner radius, and colors.

The project is in early development. The current code proves the window and
configuration foundation; the app list, clock, auto-hide, and taskbar
replacement are not implemented yet.

## Current Status

| Milestone | State |
|---|---|
| M0: GPUI window spike | Passed on Windows 10 22H2. See `docs/spike-m0.md`. |
| M1: scaffold, config, theme, logging | Implemented. |
| M2: shell window, monitor/DPI placement, live appearance | Implemented. |
| M3 onward | Not started. |

## Requirements

- Windows 10 or 11, x64.
- Rust stable with the MSVC target. `rust-toolchain.toml` pins the stable channel,
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

Quality gates used by this project:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
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

The config sections are `[appearance]`, `[behavior]`, `[clock]`, and
`[[pinned]]`.

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

- The shell window is fixed at 560x92 logical pixels. The app list and clock
  layout arrive in M3 and M4, and the content size will then follow the config.
- The Mica and Acrylic backdrop branches need Windows 11 22H2 or newer and have
  not been verified. The current machine is Windows 10 22H2. On Windows 10 the
  dock logs that the backdrop is unavailable and stays transparent.
- Monitor placement is verified in the running UI at 100% DPI. The physical to
  logical conversion and dock rectangle are unit-tested at 100%, 125%, 150%,
  and 200% DPI; a real 125% to 200% desktop was not available for manual testing.
- The native taskbar is not hidden yet. `--restore-taskbar` is not implemented;
  do not enable taskbar hiding by hand.
- The dock currently shows a shell label. App icons, the clock, auto-hide, and
  autostart arrive in later milestones.
- GPUI is pinned to `=0.2.2` because it is pre-1.0 and its API changes between
  releases.
