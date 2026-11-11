# Dockwind Handoff

Last updated: 2026-11-11
Stage: settings, tray, live theming, installer, and CI integrated. Full gate
passed on 2026-11-11. A clean-profile installer check remains.
Historical baseline: `482bd7a`

This is the starting point for a fresh agent. It records current state, pending
work, and dated evidence. It does not repeat the architecture. Read
`docs/ARCHITECTURE.md` for module ownership and lifetimes.

## Current State

| Item | Value |
|---|---|
| Product | Floating Windows 10/11 dock bar in Rust |
| Version | 0.2.0 |
| UI | GPUI `=0.2.2`, native Windows target |
| Branch | `main` |
| Source layout | Target layout integrated |
| Independent review | Cleared on 2026-11-10. No new blockers |
| Tests | 108 tests passed in the full gate on 2026-11-11 |
| Clippy | `cargo clippy --all-targets --locked -- -D warnings` passed in the final full gate |
| Intermediate gate | Historical: 59 tests passed before the lifecycle fixes. Superseded by the final gate |
| Release binary | `target\release\dockwind.exe` |
| Lints | `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks` are `deny`. Production-only `unwrap_used` and `expect_used` are denied outside tests |
| Settings | Native GPUI window in `src/ui/settings.rs` with multi-tab English layout (Appearance, Clock & Date, Behavior, Pinned Applications). One window per session via `focus_existing_settings_window`. Atomic save with `save_atomic_preserving`, dirty indicator, steppers, toggle switches, preset palettes, 1-click theme presets (Catppuccin, Frosted Glass, Nordic, OLED, Midnight), asynchronous color picker, dual-mode app picker (Start Menu scan + manual exe browse), 4-way padding, hex color input with validation, Windows 10/11 blur backdrops with DWM rounded-rect clipping (`SetWindowRgn`), alignment reference anchor (Dock vs Clock), icon alignment/spacing/style, dynamic width, drop shadow, border width/color, clock show/hide toggle, clock horizontal/vertical alignment (Top, Center, Bottom) with symmetric line gap, time and date font size steppers, custom clock strftime pattern with live cheat sheet, font picker dropdown with typography previews, font styling (family, color, bold, italic, underline), animation curve selector (Ease Out, Ease In, Ease In-Out, Linear, Ease, Bounce), trigger area height stepper, AppBar reserve desktop space offset stepper, hover lift effect on app items, and warning confirmation modal before enabling native taskbar hide. |
| Tray | Icon, menu, settings, reload, autostart, restore, quit, TaskbarCreated recovery, and a two-second visible-taskbar icon check |
| Installer | Inno Setup 6. Built and smoke-tested on 2026-11-11. Upgrade 0.1.0 to 0.2.0 passed. Start Menu and optional desktop shortcuts were created and removed. A first-run install without Dockwind data created the default config and tray icon. Uninstall while the taskbar was hidden force-stopped the app and restored the taskbar. A clean Windows profile is still pending |
| CI | GitHub Actions gate and installer artifact passed on 2026-11-11. Runs 42878351738 on `189ef25`, 42884752869 on `f8ed473`, 42889880818 on `85e4067`, 42892342420 on `c2984b5`, 42902593790 on `fc65aa3`, 42904347913 on `73ebf42`, 42906568724 on `dfd88e8`, 42911029970 on `639f945`, 42913132229 on `5fbc691`, 42914769400 on `aa9b53d`, 42928421805 on `b77667e`, and 42930790694 on `6551041` completed the `gate` and `installer` jobs. The release workflow dry run 42924851841 on `d61f871` ran the full gate, built and uploaded the installer without publishing. The tag publish run 42932966632 published the 0.2.0 release |
| Taskbar toggle | Live. Manual hide/restore, native auto-hide suspension, off-screen repositioning suppression with input disable and rect recovery marker, Explorer restart, and late `TaskbarCreated` checks passed on 2026-11-11 |
| AppBar | `reserve_space` reserves the dock strip with Win32 `SetWindowPos` geometry, independent of `hide_native_taskbar`. Work area, visible maximized-frame, and auto-hide interaction checks passed on 2026-11-11 |
| Final status | Full gate passed on 2026-11-11. Settings, tray, live geometry, live taskbar toggle, native auto-hide, AppBar reservation, the recovery CLI, Explorer restart, the basic installer path, and the release dry run manually checked. Clean-profile installer check is unverified |

The original M0 feasibility spike is closed. Do not repeat it unless the UI stack
changes. Evidence is in `docs/spike-m0.md`.

## Native Lifecycle Contract

The review-cleared source contract:

- `worker::next_message` performs a stop-aware wait, then consumes one posted
  message with `PeekMessageW`. A sent-only wake returns to the wait without a
  blocking `GetMessageW`. `WM_QUIT` returns `None` before dispatch.
- The wait polls the manual-reset stop event with `WaitForSingleObject(0)` before
  `MsgWaitForMultipleObjectsEx`. A busy queue cannot starve cancellation.
- Shutdown signals a live worker before consuming either the stop handle or the
  join handle. A signal failure returns the original error and retains both
  handles for retry. A completed worker joins without signaling. A joined panic
  or error still closes the event without aborting.
- The runtime keeps the native worker owners available for error reporting and
  retries. The logger is still the last runtime field to drop.
- Terminal cancellation failure is drop-only. If the worker is still live, the
  path calls lock-free taskbar recovery, writes a synchronous stderr diagnostic,
  and aborts. Completed workers and ordinary cleanup failures only report.
- `TaskbarOwnership` combines the state mutex with an irreversible `AtomicBool`
  recovered latch. All normal ownership checks, hides, and restores run under the
  same mutex. Recovery sets the latch before showing windows and does not take the
  state mutex.
- Every hide checks the latch before and after each native effect. An in-flight
  hide that completes after recovery compensates by force-showing its recorded
  handles. Effective ownership is the hidden flag AND `!recovered`. A late
  `TaskbarCreated` cannot hide. A startup hide after recovery returns an error.

Targeted tests use production methods with fake taskbar effects. They cover both
normal mutex interleavings, a late callback, recovery between hides, and recovery
during an in-flight effect. A message-only window test covers sent-only wake and
stop, `WM_QUIT`, and a busy queue. An injected cancellation failure at the signal
operation verifies ownership retention and retry.

## Quick Start

```powershell
cargo build
cargo run
```

Canonical local gate:

```powershell
.\scripts\check.ps1
```

The script resolves the repository root from its own path. Run it from any
directory. The final full gate ran on 2026-11-10. Run the script before and after
changes.

Runtime paths:

| Item | Path |
|---|---|
| Config | `%APPDATA%\Dockwind\config.toml` |
| Logs | `%LOCALAPPDATA%\Dockwind\logs\dockwind.log.YYYY-MM-DD` |
| Auto-hide marker | `%LOCALAPPDATA%\Dockwind\taskbar-auto-hide` |
| Manifest source | `assets/dockwind.manifest` |
| Manifest build script | `build.rs` |

CLI:

```powershell
dockwind --restore-taskbar
dockwind --enable-autostart
dockwind --disable-autostart
```

Set `RUST_LOG=dockwind=debug` for refresh, reload, and taskbar diagnostics.

## What Works

- Transparent, borderless, rounded dock drawn by GPUI. The window stays hidden
  until the first frame is drawn, so an empty white surface is not exposed.
- Primary monitor display bounds placement with DPI conversion.
- Live config reload for appearance, clock, pinned apps, and auto-hide.
- App list grouped by executable, with live icons.
- Left-click activate, restore, minimize, or launch.
- Clock and date aligned to the minute or second.
- Auto-hide with an 8 px trigger at display edge, animation, and click-through while hidden.
- Native taskbar hide and restore with a guard, panic hook, console handler,
  session-end handling, `TaskbarCreated` re-hide, and a live config toggle.
- Standalone `--restore-taskbar` recovery.
- Global quit hotkey, default `Ctrl+Alt+Shift+Q`.
- Single-instance mutex.
- Native right-click menu.
- Autostart through the menu, tray, settings window, and CLI.
- Configurable width, height, offset, opacity, and top or bottom orientation.
  Geometry changes apply live.
- Native settings window with appearance, behavior, clock, pinned app management,
  atomic save, and an external-edit prompt.
- Tray icon and menu with settings, reload, autostart, restore taskbar, and quit.
- `show_running_apps` to hide running-only apps.
- Horizontal app-list scrolling and middle-click launch for pinned apps.
- Versioned Windows resources, Inno Setup installer script, and GitHub Actions
  gate and release workflows.

These are current code capabilities. Settings, tray, live geometry, taskbar
recovery, panic and console recovery, and a real Explorer restart were manually
checked on 2026-11-11. A clean-profile installer check is still pending.

## Taskbar Safety

Read this before changing taskbar or worker code.

- `hide_native_taskbar` defaults to false. Enable it only for a controlled test.
- AppBar reservation is enabled only when both `reserve_space` and
  `hide_native_taskbar` are true. Disable it before restoring the taskbar.
- The AppBar handle removes the reservation on drop. After a forced process kill,
  run `--restore-taskbar`; the tested command restores the work area and taskbar.
- A live enable installs the panic hook, console handler, and watcher before
  hiding. A live disable restores before stopping the watcher.
- Native taskbar auto-hide is recorded once per hide and disabled with
  `ABM_SETSTATE` before the windows are hidden. Normal restore and panic/console
  recovery re-enable it when it was active. A forced kill leaves the marker at
  `%LOCALAPPDATA%\Dockwind\taskbar-auto-hide`; `--restore-taskbar` reads it,
  re-enables auto-hide, and removes it. If the marker write failed, re-enable
  auto-hide manually.
- The watcher refreshes every two seconds. It re-hides only when the taskbar
  handle set changed or native auto-hide was re-enabled, so a missed or late
  `TaskbarCreated` is covered without fighting an explicit restore.
- The tray worker verifies the icon every two seconds while `Shell_TrayWnd` is
  visible and re-adds a missing icon.
- `TaskbarRuntime` is shared on the GPUI main thread. The settings window only
  writes the config file; the config watcher applies the toggle.
- `--restore-taskbar` is handled before the single-instance check. Keep it first.
- Console handler registration is required and must succeed before the taskbar is
  hidden.
- The taskbar watcher starts before `TaskbarGuard` hides the taskbar.
- `TaskbarGuard` must stay alive through `Application::run`.
- The same state mutex guards the ownership flag, visibility records, normal
  restore, and `TaskbarCreated` re-hide. A restore that wins the lock is not
  followed by a stale hide.
- The recovered latch is irreversible. A late `TaskbarCreated` cannot hide after
  recovery. A startup hide after recovery errors.
- Every hide checks the latch before and after each native effect. An in-flight
  hide that finishes after recovery force-shows its recorded handles.
- Panic and console recovery use the lock-free force-show path. They may show a
  taskbar the user hid intentionally. That is preferred to a hidden desktop.
- The event, taskbar, and hotkey workers use the shared stop-aware reader. They do
  not depend on `PostThreadMessageW`.
- A failed stop signal retains the worker handles for retry. Do not join a live
  worker without signaling it.
- Terminal abort is limited to a drop-time cancellation failure while the worker
  is still live. Recovery and a synchronous stderr diagnostic run first.
- If a test is interrupted, run `target\release\dockwind.exe --restore-taskbar`
  or the debug equivalent immediately.
- Use a watchdog job for any taskbar-hiding test. Example: start a background job
  that runs `dockwind --restore-taskbar` after 40 seconds and cancel it when the
  test succeeds.

## Evidence

### Historical evidence, baseline `482bd7a`

| Date | Area | Evidence |
|---|---|---|
| 2026-11-10 | M0 window | Transparent rounded popup, tool window, topmost, no activate, click input, cross-thread UI wake. See `docs/spike-m0.md` |
| 2026-11-10 | Config | Defaults created, partial config accepted, invalid values warned and defaulted, corrupt reload keeps the last valid config. See `docs/decisions.md` D004 |
| 2026-11-10 | Appearance | Live background and radius changes. Mica request logs unsupported on Windows 10. See D009 |
| 2026-11-10 | Clock | Minute boundary and seconds format. See D010 |
| 2026-11-10 | App list | Live icons, launch, activate, minimize, close, relaunch, event refresh. See D011 to D013 |
| 2026-11-10 | Auto-hide | Hidden window is click-through, bottom trigger shows, leaving hides, live switch. See D014 and D015 |
| 2026-11-10 | Taskbar | Normal quit restored, kill plus restore command recovered, simulated `TaskbarCreated` re-hid, single instance rejected a second process. See D016 and D017 |
| 2026-11-10 | Menu | Opened, Escape dismissed, reload worked, autostart toggled, restore and quit exited. See D019 and D020 |
| 2026-11-10 | Autostart | Registry value created and removed through the menu and CLI |
| 2026-11-10 | Quality | 36 tests, fmt, clippy, and release build passed at `482bd7a` |

Historical evidence does not automatically verify the integrated code.

### Current integrated code

| Date | Area | Evidence |
|---|---|---|
| 2026-11-10 | Config module split | 22 isolated module tests passed. Atomic replacement reload, missing-file watch, relative path, unrelated-file filtering, debounce, shutdown under sustained events, and receiver-drop shutdown are covered. `WatcherPanicked` is additive but has no worker-panic or failed-join test. See D027 |
| 2026-11-10 | Intermediate gate | Full `scripts/check.ps1` passed before the lifecycle fixes: fmt, check, clippy, 59 tests, release build, link checks. Not the final gate |
| 2026-11-10 | Native lifecycle fixes | 26 targeted Windows tests passed after the fixes. Sent-only wake, busy queue, `WM_QUIT`, cancellation retention, normal mutex interleavings, late callback, and recovery during an in-flight hide are covered. `cargo clippy --all-targets --locked -- -D warnings` passed. Superseded by the final gate |
| 2026-11-10 | Final full gate | `scripts/check.ps1` passed on the final review-cleared source: fmt, check, clippy with `-D warnings`, **66 tests**, release build, link parser self-test (16 cases), and 27 Markdown links in 11 files |
| 2026-11-10 | Startup surface fix | Release binary launched with the window hidden and then shown. Log recorded `dock window shown after first frame`. Hidden state was transparent; shown state rendered icons and the clock. The full gate passed after the fix: fmt, check, clippy with `-D warnings`, **66 tests**, release build, link parser, and Markdown links |
| 2026-11-11 | Live theming and geometry | A config reload changed the width to 700 and the orientation to top. The running dock moved to the top. Restoring 560 and bottom moved it back. Unit tests cover top placement, the top trigger, opacity scaling, and size bounds. 100% DPI |
| 2026-11-11 | Settings window | Opened through `--settings`; every field matched the config. Apply saved a valid width. Close discarded a draft width of 700 and left the file at 560. Width `abc` produced `width must be a whole number: invalid digit found in string` and left the file unchanged. A new quit hotkey saved and showed `Restart Dockwind to apply the new quit hotkey.` |
| 2026-11-11 | Settings external edit | With the settings window open, the file was changed to width 620 with an extra comment. Apply showed `The config file changed on disk. Overwrite it?`. No preserved the external file. A second Apply and Yes overwrote it with 560 and dropped the comment |
| 2026-11-11 | Settings single window | The first `--settings` process owned one `DockwindSettings` window. A second `--settings` process exited 0 without creating a window, leaving one window. After the first closed, a third process opened a new window. This repeated across separate processes |
| 2026-11-11 | Settings save preservation | Unit tests wrote comments, an unknown top-level key, and an unknown table. Apply changed width to 700 and preserved all of them. A legacy `margin_bottom` was removed when `offset` was written, pinned entries round-tripped, and an invalid existing TOML was rejected without changing the file |
| 2026-11-11 | Settings pinned apps | Added `notepad.exe` through the file picker, then Apply wrote one pinned entry. With two entries, Down reordered them and Apply wrote Calculator first. Selecting the first entry and Remove left one Notepad entry |
| 2026-11-11 | App list and auto geometry | Horizontal scrolling and middle-click launch were added. A one-second monitor poll repositions the dock on work-area, display, or DPI changes. The dock still rendered at a 300 px width. Resolution and DPI changes and the middle-click gesture were not manually exercised |
| 2026-11-11 | Taskbar startup and quit | With a 45-second watchdog, startup with `hide_native_taskbar = true` hid `Shell_TrayWnd`. `Ctrl+Alt+Shift+Q` was received, the process exited, and the taskbar was visible again. The log recorded `quit hotkey received` and `native taskbar restored`. `ABM_GETSTATE` went from 1 to 0 while hidden and back to 1 after quit |
| 2026-11-11 | Native auto-hide | The native taskbar was set to auto-hide (`ABM_GETSTATE = 1`) and `Shell_TrayWnd` was visible. With `hide_native_taskbar = true`, the state became 0 and `Shell_TrayWnd` became hidden. After quit the state returned to 1 and the taskbar was visible |
| 2026-11-11 | Panic restore | A controlled panic on a debug build with `hide_native_taskbar = true` restored auto-hide to 1 and made `Shell_TrayWnd` visible while the process stayed alive. The marker remained until normal quit, which removed it. The temporary debug test hook was removed from source |
| 2026-11-11 | Console control | Dockwind started with `CREATE_NEW_CONSOLE` and `CREATE_NEW_PROCESS_GROUP` hid the taskbar. A helper attached to its console and sent `CTRL_BREAK_EVENT`. The target exited; auto-hide returned to 1 and the taskbar was visible. `--restore-taskbar` removed the remaining marker |
| 2026-11-11 | Auto-hide marker recovery | While hiding, `%LOCALAPPDATA%\Dockwind\taskbar-auto-hide` contained `1` and auto-hide was 0. A forced kill left both unchanged. `--restore-taskbar` returned auto-hide to 1, removed the marker, and showed the taskbar. A second run and normal quit also restored auto-hide 1 and removed the marker |
| 2026-11-11 | Explorer restart | A real `explorer.exe` restart replaced all three taskbar windows. The two-second watcher refresh re-hid the replacement within about six seconds and set auto-hide to 0. The late `TaskbarCreated` broadcast only refreshed the state. The tray icon was re-added and resolved with `Shell_NotifyIconGetRect` after `--restore-taskbar`. Normal quit restored auto-hide 1 and left the taskbar visible without a restore error |
| 2026-11-11 | AppBar reservation | With `hide_native_taskbar = true`, `reserve_space = true`, `auto_hide = false`, and a 50-second restore watchdog, the work area became `0,0,1920,976` and the dock was placed at `y=976`. `Shell_TrayWnd` was hidden. After `Ctrl+Alt+Shift+Q`, the process exited, the work area returned to `0,0,1920,1040`, and the taskbar was visible. A maximized Notepad had a visible DWM frame of `0,0,1920,976`; its `GetWindowRect` was `984` because of the 8 px invisible border |
| 2026-11-11 | Installer upgrade | `Dockwind-0.1.0-setup.exe` installed silently. `Dockwind-0.2.0-setup.exe` installed over it with exit 0. The installed `--settings` window title read `Dockwind Settings 0.2.0`. Silent uninstall removed the install directory and Start Menu shortcut and preserved the config |
| 2026-11-11 | AppBar with auto-hide | Native auto-hide was 1 and the work area was `0,0,1920,1080`. With `reserve_space = true`, auto-hide became 0, the AppBar work area was `0,0,1920,976`, and `Shell_TrayWnd` was hidden. After quit auto-hide returned to 1, the work area returned to `0,0,1920,1080`, and the taskbar was visible |
| 2026-11-11 | AppBar forced kill | A forced process kill while reserving space left the work area at `0,0,1920,976`. It stayed there through a 35-second poll. Running `--restore-taskbar` returned it to `0,0,1920,1080`, showed the taskbar, and removed the marker. The recovery command is required |
| 2026-11-11 | Live taskbar toggle | With a 40-second `--restore-taskbar` watchdog, a config reload changed `hide_native_taskbar` from false to true and `IsWindowVisible(Shell_TrayWnd)` went from true to false. Changing it back restored visibility. The settings checkbox plus Apply repeated the same sequence. The log recorded `taskbar mode updated`, `native taskbar hidden`, and `native taskbar restored` |
| 2026-11-11 | Hosted CI | GitHub Actions `CI` run 42866275074 on `adb2206` and run 42868103165 on `42421fc` both completed successfully on `windows-latest`. Runs 42878351738 on `189ef25`, 42884752869 on `f8ed473`, 42889880818 on `85e4067`, 42892342420 on `c2984b5`, 42902593790 on `fc65aa3`, 42904347913 on `73ebf42`, 42906568724 on `dfd88e8`, 42911029970 on `639f945`, 42913132229 on `5fbc691`, 42914769400 on `aa9b53d`, 42928421805 on `b77667e`, and 42930790694 on `6551041` completed both `gate` and `installer` jobs and uploaded the `Dockwind-installer` artifact |
| 2026-11-11 | Release dry run | Manual `release.yml` dispatch run 42924851841 on `d61f871` resolved `0.2.0` from `Cargo.toml`, ran the full quality gate, built the installer, and uploaded the `Dockwind-0.2.0-setup` artifact (4,259,713 bytes). No GitHub Release was created. The tag path was validated later |
| 2026-11-11 | Release publish | Annotated tag `v0.2.0` on `a92e5b0` triggered release run 42932966632. The build job passed and published `Dockwind 0.2.0` at https://github.com/jshsinaga/dockwind/releases/tag/v0.2.0 with asset `Dockwind-0.2.0-setup.exe` (4,775,376 bytes) |
| 2026-11-11 | Tray | Icon visible in the notification area. Right-click showed Settings, Reload config, Start with Windows, Restore taskbar, and Quit. The Settings item opened the settings window |
| 2026-11-11 | UWP mapping | Settings opened an `ApplicationFrameHost` frame with a `Windows.UI.Core.CoreWindow` child owned by `SystemSettings.exe`. The debug log mapped frame PID 11608 to app PID 6764, path `SystemSettings.exe`, name `Settings`; the app list refreshed 5 items. The visual icon was not checked |
| 2026-11-11 | Startup bounds and shadow removal | Applied startup set_bounds, show: true in WindowOptions for immediate SetWindowPlacement calculation, and DWM border/shadow removal (DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, WS_POPUP, DWMWCP_DONOTROUND). Startup window dimensions match configured width/height without requiring a click. Unit tests in win::dwm passed |
| 2026-11-11 | Taskbar-relative dock and VSync slide animation | Restored dock offset relative to native taskbar via work_area while trigger zone spans from taskbar top down to display bottom. Slide animation uses VSync frame requests via window.request_animation_frame() with quadratic ease-out at the monitor refresh rate, removing opacity fade (D046). Unit tests pass |
| 2026-11-11 | Shutdown latency | Three release runs with `auto_hide = false` exited in 98 ms, 77 ms, and 77 ms after `Ctrl+Alt+Shift+Q` |
| 2026-11-11 | Pinned apps overhaul | Replaced in-app installed apps modal with native Windows file picker (`pick_file_async`). Shell icons extracted via `icon_from_path` with `RefCell` caching and displayed in list. Added direct row actions for Move Up (`^`), Move Down (`v`), and Delete (`X`). Added delete confirmation dialog modal. Implemented drag-and-drop reordering with drag handle (`::`) and `on_drop` / `drag_over`. Full gate passed with **109 tests** |
| 2026-11-11 | Full gate | `scripts/check.ps1` passed on the current source: fmt, check, clippy with `-D warnings`, **109 tests**, release build, link parser self-test (16 cases), and 35 Markdown links in 13 files |
| 2026-11-11 | Installer | `installer/dockwind.iss`, `scripts/build-installer.ps1`, and `scripts/bump-version.ps1` produced the final `dist\Dockwind-0.2.0-setup.exe` with Inno Setup 6.7.3. Silent 0.1.0 install and 0.2.0 upgrade exited 0. The installed settings title read `Dockwind Settings 0.2.0`. The Start Menu shortcut existed after install and was removed by silent uninstall. The install directory was removed and the config was preserved. The installer was rebuilt after the TOML preservation, settings-mutex, marker-test, UWP mapping, and uninstall force-stop changes; silent install and uninstall exited 0. A first-run install with no existing Dockwind config started the app, created the default config, and added the tray window; uninstall kept the config. An install with the desktop-shortcut task created both shortcuts. Uninstall while the taskbar was hidden force-stopped the app, restored auto-hide 1, showed the taskbar, removed the install directory and both shortcuts, and preserved the config. A clean Windows profile is still untested |
| 2026-11-11 | Build resources | `build.rs` injects the package version into the manifest and embeds the manifest and icon. The release build passed |
| 2026-11-10 | Build warning | Baseline `proc-macro-error2` v2.0.1 warning remains |
| 2026-11-10 | Earlier debug startup smoke | 4 seconds on Windows 10 build 19045, 1920x1080, 100% DPI, `hide_native_taskbar = false`. Logs showed config load, hotkey registration, placement, transparent styles. Process was force-killed |

Not verified on the current integrated code:

- Clean-profile install and Start Menu shortcut creation. The upgrade, uninstall, and shortcut removal path passed.
- Windows 11 behavior.
- Mica or Acrylic.
- Real multi-DPI desktop.

Startup geometry, first-frame DirectComposition presentation without user click,
and borderless popup styles were manually verified on 2026-11-11 on Windows 10
(build 19045).

The full gate passed on 2026-11-11. The evidence above applies to the current
source.

## Pending Work

Order is a recommendation, not a commitment.

1. **Clean-profile installer check.** Run the 0.2.0 installer on a fresh Windows
   profile, confirm the Start Menu and optional desktop shortcuts, and confirm
   config preservation. The first-run install with no Dockwind data, upgrade,
   launch, and uninstall path passed on 2026-11-11. See PRD P08 to P13 and
   `docs/RELEASE.md`.
2. **Automatic geometry check.** Exercise a resolution change, a DPI change, and a
   taskbar hide or restore. The dock polls the primary monitor every second and
   repositions itself. See PRD U02.
3. **App-list polish.** Add tooltips and a clearer overflow affordance.
   Middle-click launch and horizontal scrolling are implemented. See PRD U03.
4. **Windows 11 blur.** Exercise Mica and Acrylic on Windows 11 22H2 or newer.
   See PRD U05.
5. **Multi-DPI UI.** Exercise the running UI at 125%, 150%, and 200%. See PRD U07.
6. **Settings polish.** Inline field validation.
7. **Code signing.** The installer and binary are unsigned. Add signing only with
   a certificate and a recorded decision.

## Known Limitations

The detailed list is in `docs/PRD.md` section 5 and `docs/ARCHITECTURE.md`
section 9. The short version:

- Config geometry is live. DPI, display, and work-area changes are polled once per
  second; a real multi-DPI or resolution change was not exercised.
- `reserve_space` requires hiding the native taskbar. A forced process kill can
  leave the reserved work area until `--restore-taskbar` runs.
- Native taskbar auto-hide is suspended while the taskbar is hidden. A forced
  process kill leaves it disabled until `--restore-taskbar` runs; that command
  restores the recorded state from the marker file.
- The app list scrolls horizontally and supports middle-click launch for pinned
  apps. Tooltips are not implemented.
- UWP frame hosts map to the hosted child process for identity and name; the
  icon is not visually checked.
- `clock.locale` is unused.
- Mica and Acrylic are unverified on Windows 11.
- The running UI was exercised at 100% DPI only.
- The installer is built and the first-run, 0.1.0 to 0.2.0 upgrade, install, and
  uninstall path passed. A clean Windows profile was not tested. The installer is
  unsigned. Release `v0.2.0` is published with the installer asset.
- A settings save preserves comments, ordering, and unknown keys for a valid
  file, but rewrites `pinned` entries. An invalid TOML file is not overwritten.
  The native window has message box errors, not inline field errors. A second
  settings process focuses the existing window. The taskbar toggle is live and
  the settings checks passed.
- A missing tray icon is logged and does not block the dock.

## Documentation Map

| File | Role |
|---|---|
| [AGENTS.md](../AGENTS.md) | Authoritative working rules and spec pointers |
| [docs/PRD.md](PRD.md) | Scope, requirements, status, acceptance criteria |
| [docs/ARCHITECTURE.md](ARCHITECTURE.md) | Target modules, ownership, lifetimes, invariants |
| [DESIGN.md](../DESIGN.md) | UI and window patterns |
| [docs/CONFIGURATION.md](CONFIGURATION.md) | Config schema, validation, reload, save contract |
| [docs/RELEASE.md](RELEASE.md) | Versioning, installer, and GitHub Actions |
| [CHANGELOG.md](../CHANGELOG.md) | User-facing changes by release |
| [docs/engineering.spec.md](engineering.spec.md) | Rust and verification conventions |
| [docs/decisions.md](decisions.md) | D001 onward, including the organization decisions |
| [docs/HANDOFF.md](HANDOFF.md) | This handoff: state, pending work, dated evidence |
| [docs/spike-m0.md](spike-m0.md) | Historical M0 evidence |
| [docs/implementation_plan.md](implementation_plan.md) | Historical original plan. Not current scope |
| [README.md](../README.md) | User-facing setup and behavior |
| [scripts/check.ps1](../scripts/check.ps1) | Canonical local gate and Markdown link check |

## First Hour for a Fresh Agent

1. Read `AGENTS.md`, `docs/PRD.md`, `docs/ARCHITECTURE.md`,
   `docs/CONFIGURATION.md`, and recent entries in `docs/decisions.md`.
2. Read Native Lifecycle Contract before touching source.
3. Run `git status` and `.\scripts\check.ps1` before changing anything. The
   2026-11-11 gate applies to that source. Later runs are gate evidence for your
   own changes, not that source.
4. Launch with `hide_native_taskbar = false` and `auto_hide = false` for a
   predictable UI. Enable auto-hide only when testing it.
5. Open the settings window with `--settings` or the tray. The tray and settings
   window are safe to use with the taskbar visible.
6. Pick one pending item. Write a pure test where possible. Verify manually.
7. Update the canonical doc and this handoff with dated evidence.
8. Never leave the taskbar hidden. Use the restore command and a watchdog during
   any taskbar-hiding test.
