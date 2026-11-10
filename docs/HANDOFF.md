# Dockwind Handoff

Last updated: 2026-11-10

This is the starting point for a fresh agent. It records the current state, how to
run and verify the app, the architecture, the safety rules around the native
taskbar, and the remaining work.

## Current State

| Item | Value |
|---|---|
| Product | Floating Windows 10/11 dock bar in Rust |
| UI | GPUI `=0.2.2`, native Windows target |
| Git branch | `main` |
| Latest code milestone | `cfe72cc` (M7 context menu and autostart toggle) |
| Milestones | M0 through M6 complete; M7 implemented except the gaps below |
| Tests | 36 passing |
| Release binary | `target\release\dockwind.exe` |
| Build status | `cargo fmt --check`, `clippy -D warnings`, `cargo test`, and release build pass |

M0 through M7 were implemented sequentially. Do not redo the M0 feasibility spike;
its evidence is in `docs/spike-m0.md`.

## Quick Start

```powershell
cargo build
cargo run
```

Quality gates:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Runtime paths:

| Item | Path |
|---|---|
| Config | `%APPDATA%\Dockwind\config.toml` |
| Logs | `%LOCALAPPDATA%\Dockwind\logs\dockwind.log.YYYY-MM-DD` |
| Manifest source | `assets/dockwind.manifest` |
| Manifest build script | `build.rs` |

Useful CLI:

```powershell
dockwind --restore-taskbar
dockwind --enable-autostart
dockwind --disable-autostart
```

Set `RUST_LOG=dockwind=debug` for refresh, reload, and taskbar diagnostics.

## What Works

- Transparent, borderless, rounded dock drawn by GPUI.
- Monitor work-area placement with DPI conversion at 100% to 200% (unit-tested).
- Config hot reload for appearance, clock formats, pinned apps, and auto-hide.
- App list grouped by executable, with live icons from window/class/exe.
- Left-click activates, restores, or minimizes a running app.
- Pinned app launches when not running.
- Clock and date, aligned to the minute or second based on the format.
- Auto-hide with a 2 px work-area trigger, animation, and click-through while hidden.
- Native taskbar hide/restore with a drop guard, panic hook, console handler,
  session-end handling, and `TaskbarCreated` re-hide.
- Standalone `--restore-taskbar` recovery.
- Global quit hotkey, default `Ctrl+Alt+Shift+Q`.
- Single-instance mutex.
- Right-click native menu: Reload config, Enable/Disable autostart, Restore
  taskbar & Quit.
- Autostart through the menu and CLI.

## Architecture

| Path | Responsibility |
|---|---|
| `src/main.rs` | CLI commands, logging, config load, single instance, taskbar guard, hotkey, GPUI start |
| `src/config.rs` | Config defaults, parsing, validation, path, hot reload, watch channel |
| `src/theme.rs` | Colors, contrast text, radius, padding |
| `src/autohide.rs` | Pure auto-hide state machine with injected time |
| `src/app_state.rs` | Pure pinned/running merge and `DockItem` grouping |
| `src/ui/mod.rs` | Window creation, monitor placement, initial backdrop |
| `src/ui/dock.rs` | Dock state, app list, config updates, auto-hide polling, menu, clicks |
| `src/ui/app_item.rs` | App icon and running/active indicator |
| `src/ui/clock.rs` | Clock/date formatting and boundary timer |
| `src/win/monitors.rs` | Primary monitor work area, DPI conversion, dock/trigger rectangles |
| `src/win/dwm.rs` | Extended styles, topmost, no-activate, backdrop, click-through, HWND helpers |
| `src/win/windows_enum.rs` | Window enumeration/filtering, process path, activate/minimize |
| `src/win/icons.rs` | HICON to GPUI `RenderImage`, icon cache |
| `src/win/events.rs` | WinEvent hook thread and event channel |
| `src/win/launch.rs` | `ShellExecuteW` launcher and argument quoting |
| `src/win/cursor.rs` | Cursor position and fullscreen detection |
| `src/win/taskbar.rs` | Taskbar hide/restore, recovery handlers, taskbar watcher |
| `src/win/hotkey.rs` | Hotkey parsing, registration, quit event |
| `src/win/menu.rs` | Native right-click menu |
| `src/win/autostart.rs` | HKCU Run key enable/disable/query |
| `src/win/single_instance.rs` | Named mutex guard |

Data flow:

1. `win-events` thread sends window changes; `DockView` debounces for 120 ms and
   re-enumerates the app list.
2. Config watcher sends new `Config`; `DockView::apply_config` updates theme,
   clock, pinned apps, auto-hide, and backdrop.
3. Auto-hide polling sends cursor samples to the pure state machine, which drives
   progress and `WS_EX_TRANSPARENT`.
4. Right-click opens a native menu on a deferred foreground task; `menu_open`
   keeps the dock visible.
5. `TaskbarGuard` owns the hidden taskbar lifecycle and restores on drop.

## Taskbar Safety

Read this before changing `src/win/taskbar.rs` or `src/main.rs`.

- `hide_native_taskbar` defaults to `false`. Enable it only for a controlled test.
- The guard is created after `install_watch()` and before the app starts. Keep
  that order.
- `TaskbarGuard` must stay alive through `Application::run`.
- `--restore-taskbar` is handled before the single-instance check and force-shows
  all taskbar windows. Keep it first in `main`.
- Panic, console control events, and `WM_ENDSESSION` restore the saved-visible
  taskbar. `TaskbarCreated` re-hides a taskbar after Explorer restarts.
- If a test is interrupted, run `target\release\dockwind.exe --restore-taskbar`
  or the debug equivalent immediately.
- Use a watchdog job when testing taskbar hiding. Example: start a background job
  that runs `dockwind --restore-taskbar` after 40 seconds and cancel it when the
  test succeeds.

## Verified

| Area | Evidence |
|---|---|
| M0 window | Transparent rounded popup, toolwindow/no appwindow, topmost, noactivate, click input, cross-thread UI wake |
| Config | Defaults created, partial config accepted, invalid values warned/defaulted, corrupt reload keeps last valid |
| Appearance | Live background and radius changes; Mica request logs unsupported on Windows 10 |
| Clock | Minute boundary 17:28 to 17:29; seconds format updates every second |
| App list | Live icons; launch, activate, minimize, close, relaunch; event refresh 8-13 ms after debounce |
| Auto-hide | Hidden window is click-through; bottom trigger shows; leaving hides; live auto_hide switch |
| Taskbar | Normal quit restores; kill plus restore command recovers; simulated TaskbarCreated re-hides; single instance rejects second process |
| Menu | Opens, Escape dismisses, reload works, autostart toggles on/off, restore-and-quit exits |
| Autostart | Registry value created and removed through menu and CLI |
| Quality | 36 tests, fmt, clippy, release build |

## Known Gaps and Recommended Order

1. **Work area after hiding the taskbar.** Hiding the taskbar does not change
   `rcWork`, so the dock stays 12 px above the old work-area edge instead of the
   screen bottom. Fix by using display bounds when `hide_native_taskbar` is true,
   or by implementing AppBar/`SPI_SETWORKAREA` handling. Re-test the trigger strip.
2. **AppBar `reserve_space`.** Not implemented. `SHAppBarMessage` is still needed
   for the optional reserved-space mode.
3. **App-list polish.** Middle-click new instance, tooltips, and scrolling or
   overflow for many apps are missing. The dock content is fixed at 560x92.
4. **UWP identity.** `ApplicationFrameHost.exe` windows are skipped. A real map
   needs the child app's AppUserModelID or a different identity source.
5. **Windows 11 blur.** Mica/Acrylic and DWM corner preference need a Windows 11
   22H2+ machine. The code gates on build 22621.
6. **Manual robustness tests.** A real Explorer restart, panic restore,
   console-control restore, and real fullscreen suppression were not exercised.
7. **Shutdown latency.** Quit works but the process exits several seconds after
   the request. Profile GPUI/DirectX/icon teardown if it matters.
8. **Multi-DPI UI.** Only 100% was exercised in the running UI. 125%, 150%, and
   200% are unit-tested conversions.
9. **Startup-only settings.** `hide_native_taskbar` and `quit_hotkey` require a
   restart to change.

## Documentation Map

| File | Role |
|---|---|
| `AGENTS.md` | Authoritative working rules |
| `docs/implementation_plan.md` | Original scope, architecture, milestones, verification |
| `README.md` | User-facing build, config, CLI, limitations |
| `docs/decisions.md` | D001-D020 design decisions and deviations |
| `docs/spike-m0.md` | Historical M0 feasibility evidence |
| `docs/engineering.spec.md` | Extracted engineering rules and current-state summary |
| `docs/HANDOFF.md` | This handoff and continuation guide |

`DESIGN.md`, `docs/PRD.md`, `docs/ARCHITECTURE.md`, and `docs/ERD.md` are still
absent. Do not create or move files to satisfy those references without asking.

## First Hour for a Fresh Agent

1. Read `AGENTS.md`, `docs/HANDOFF.md`, the relevant sections of
   `docs/implementation_plan.md`, and recent entries in `docs/decisions.md`.
2. Run `git status` and all four quality gates before changing anything.
3. Launch with `hide_native_taskbar = false` and `auto_hide = false` for a
   predictable UI, then enable auto-hide when testing it.
4. Pick one gap, write a pure test where possible, verify manually, update
   `docs/decisions.md`, and commit one logical change.
5. Never leave the taskbar hidden. Use the restore command and a watchdog during
   any taskbar-hiding test.
