# Dockwind Product Requirements

Status: active. This document defines what Dockwind builds and why. It owns scope
and acceptance criteria. It does not own architecture or configuration details.

Last updated: 2026-11-11
Current code: integrated, review-cleared, and extended with settings, tray, theming,
and release delivery
Current stage: full gate on 2026-11-11 after the settings, tray, live geometry,
auto-hide, Explorer, panic, and console recovery work. A clean-profile install
and the tag release remain manual.
Historical baseline: `482bd7a`

## 1. Product Definition

Dockwind is a floating dock bar for Windows 10 and 11. It shows pinned and running
apps, a clock and date, and optional auto-hide behavior. It can hide the native
Windows taskbar and draw its own dock with configurable transparency, radius, and
colors.

Dockwind hides `Shell_TrayWnd` and `Shell_SecondaryTrayWnd`. It does not stop
`explorer.exe`. Start, the system tray, the notification center, and notification
badges remain Windows features and are not replaced.

## 2. Users and Jobs

| User | Job |
|---|---|
| Desktop user | Reach pinned and running apps from one bottom bar |
| Keyboard user | Keep the current app focused while using the dock with the mouse |
| User who hides the taskbar | Recover the native taskbar at any time, including after a crash |
| User who edits config | Change appearance and behavior without restarting for live fields |

## 3. Status Vocabulary

| Status | Meaning |
|---|---|
| Verified | Code exists and dated evidence covers the claim |
| Implemented | Code exists, but at least one required manual check is missing or environment-limited |
| Accepted unfinished | In current scope, not implemented |
| Proposed | Future work. Not approved for the current release |

A behavior is not Verified only because a code path exists. Evidence lives in
`docs/HANDOFF.md`, `docs/spike-m0.md`, or a linked decision.

## 4. Requirements

### 4.1 Dock Surface

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R01 | Show a borderless, transparent dock at the bottom of the primary display | Verified | The dock has no native title bar, no taskbar button, and no Alt+Tab entry. The desktop remains visible through the rounded surface |
| R02 | Place the dock against the primary monitor work area | Implemented | The dock is centered and above the work-area edge at 100% DPI. Rect and DPI conversion pass at 100%, 125%, 150%, and 200%. A real 125% to 200% desktop is not yet exercised |
| R03 | Keep the dock non-activating | Verified | Clicking the dock does not take focus from the foreground app. The previous foreground window is restored after creation |
| R04 | Keep the dock above normal windows | Verified | `WS_EX_TOPMOST` and `SetWindowPos(HWND_TOPMOST)` are applied and observed |

### 4.2 Configuration

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R05 | Create a default config on first run | Verified | `%APPDATA%\Dockwind\config.toml` is created with every field. A partial file parses with defaults |
| R06 | Validate configuration values | Verified | Invalid color, icon size, radius, clock format, and padding overflow are replaced with safe values and logged. The app does not crash |
| R07 | Reload live configuration | Implemented | Saving the file applies appearance, clock, pinned apps, auto-hide settings, fullscreen hiding, and blur after the debounce. A corrupt reload keeps the last valid config. Baseline live behavior was verified at `482bd7a`. The integrated watcher has automated reload and shutdown tests, but no new live UI check |
| R08 | Keep startup-only settings explicit | Verified | `quit_hotkey` applies only at startup. `hide_native_taskbar` and `offset` are live. The docs state it. The settings window shows a restart notice for the hotkey |
| R09 | Keep existing TOML keys compatible | Verified | Existing keys, sections, and defaults remain valid. Unknown keys are ignored on load |

### 4.3 Apps and Clock

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R10 | Show pinned apps in config order, then other running apps | Verified | Pinned apps keep their configured order. Running apps that are not pinned are appended |
| R11 | Group windows by executable | Verified | Two windows from the same executable produce one dock item. Case differences in the path do not split the group |
| R12 | Show live app icons | Verified | Icons come from the live window, then the window class, then the executable. The fallback letter is shown when no icon exists |
| R13 | Activate, restore, minimize, and launch from the dock | Verified | Clicking a background app brings it forward. Clicking the foreground app minimizes it. Clicking a pinned app that is not running launches it |
| R14 | Show time and date from config formats | Verified | The clock changes at the minute boundary. Formats with seconds change every second. Invalid formats are rejected at load |
| R15 | Keep the clock aligned without fixed polling | Verified | The timer targets the next minute or second boundary, not a fixed short interval |

### 4.4 Auto-Hide

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R16 | Show the dock when the pointer reaches the bottom work-area edge | Verified | A 2 px trigger strip shows the dock after the show delay. The dock hides after the hide delay when the pointer leaves |
| R17 | Avoid flicker on a quick pointer pass | Verified | The state machine does not show the dock on a short pass. A unit test covers the state transition |
| R18 | Make the hidden dock click-through | Verified | At zero progress, `WS_EX_TRANSPARENT` is set and the invisible window does not intercept clicks |
| R19 | Keep the dock visible while the context menu is open | Verified | `menu_open` pins the dock. The menu can be dismissed with Escape |
| R20 | Hide the dock during fullscreen apps | Implemented | `SHQueryUserNotificationState` drives `force_hidden`. The state machine is unit-tested. A real fullscreen game or video is not yet exercised |

### 4.5 Native Taskbar Safety

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R21 | Hide the native taskbar only when configured | Verified | `hide_native_taskbar` defaults to false. The hide path is not entered by default. While hidden, Dockwind suspends native taskbar auto-hide and restores the previous state on normal restore |
| R22 | Restore the taskbar on normal quit | Verified | The `TaskbarGuard` restores the saved-visible taskbar windows and the recorded auto-hide state. On 2026-11-11 a watchdog-backed startup hide and `Ctrl+Alt+Shift+Q` quit restored `Shell_TrayWnd`; `ABM_GETSTATE` returned from 0 to 1 |
| R23 | Restore the taskbar on panic and console control events | Verified | The panic hook and console handler use the lock-free force-show path with the irreversible recovered latch. On 2026-11-11 a controlled panic restored the taskbar and auto-hide while the process stayed alive. A `CTRL_BREAK_EVENT` to a new process group restored auto-hide 1 and showed the taskbar as the target exited |
| R24 | Restore the taskbar on session end and Explorer restart | Verified | `WM_ENDSESSION` and a real Explorer restart use the ownership path. On 2026-11-11 a replacement taskbar was detected within two seconds, auto-hide was disabled again, and the taskbar was hidden. A late `TaskbarCreated` refreshed without re-hiding. The tray icon returned |
| R25 | Provide a standalone recovery command | Verified | `dockwind --restore-taskbar` force-shows all taskbar windows and runs before the single-instance check. It restored the taskbar and work area after a forced kill on 2026-11-11. It also restored the native auto-hide state from the marker file and removed the marker |
| R26 | Prevent two dock instances | Verified | A named mutex rejects a second instance. The restore command bypasses the mutex |
| R27 | Provide a global quit hotkey | Verified | The default `Ctrl+Alt+Shift+Q` registers at startup. The 2026-11-11 watchdog check received the hotkey, exited the app, and restored the taskbar |

### 4.6 Integrations

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R28 | Provide a native right-click menu | Verified | The menu opens, Escape dismisses it, config reload works, autostart toggles, and Restore taskbar and Quit exits |
| R29 | Support per-user autostart | Verified | The menu and `--enable-autostart` / `--disable-autostart` create and remove the HKCU Run value |
| R30 | Keep the CLI usable | Implemented | The binary supports `--restore-taskbar`, `--enable-autostart`, `--disable-autostart`, and `--settings`. The recovery command remains first and bypasses the single-instance check |
| R31 | Keep file locations stable | Verified | Config stays at `%APPDATA%\Dockwind\config.toml`. Logs stay at `%LOCALAPPDATA%\Dockwind\logs` |

### 4.7 Theming, Settings, Tray, and Delivery

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R32 | Configure dock geometry and orientation | Implemented | `width`, `height`, `offset`, and `orientation` (top or bottom) are validated. Config changes reposition and resize the running dock without a restart. Top and bottom placement are manually checked at 100% DPI |
| R33 | Configure transparency and appearance | Implemented | `opacity` multiplies the background alpha from 10 to 100. Background, radius, icon size, padding, and opacity apply live. Invalid values are sanitized and logged |
| R34 | Provide a settings window | Verified | A separate focusable window edits appearance, behavior, clock, and pinned apps. Apply validates and saves atomically. Close discards unsaved edits. External file changes prompt before overwrite. On 2026-11-11 the validation, prompt, restart notice, Close discard, and pinned add/remove/reorder checks passed. The window is native Win32, not GPUI |
| R35 | Provide a tray icon | Verified | A tray icon exists while the dock runs. Its menu opens settings, reloads config, toggles autostart, restores the taskbar, and quits. Double-click opens settings. On 2026-11-11 the icon returned after a real Explorer restart, and a two-second visible-taskbar check re-adds a missing icon |
| R36 | Manage displayed apps | Implemented | The settings window adds, removes, and reorders pinned apps. `show_running_apps = false` hides running-only apps |
| R37 | Package a release | Implemented | An Inno Setup script builds a per-user installer with Start Menu and optional desktop shortcuts. Uninstall restores the taskbar and disables autostart. Built and smoke-tested on 2026-11-11. A first-run install without Dockwind data and the 0.1.0 to 0.2.0 upgrade passed; a clean Windows profile is not checked |
| R38 | Provide hosted CI and release automation | Implemented | GitHub Actions runs the local gate on Windows for pushes and pull requests. The gate and installer artifact job passed on 2026-11-11. A manual dispatch dry run built and uploaded the 0.2.0 installer without publishing. A `vX.Y.Z` tag checks the version and publishes a GitHub release; the tag publish path has not run yet |
| R39 | Toggle the native taskbar without restart | Verified | A live config change from false to true hides `Shell_TrayWnd` and from true to false restores it. The panic hook, console handler, and watcher start before hiding. Manual check on 2026-11-11 with a watchdog |
| R40 | Reserve screen space with an AppBar | Verified | With `hide_native_taskbar = true` and `reserve_space = true`, an AppBar reservation shrinks the work area to stop at the dock top. A maximized window visible frame ends at the work area. `ABM_REMOVE` restores it on quit. A forced kill needs `--restore-taskbar`; the shell did not clear the reservation within 35 seconds |

## 5. Accepted Unfinished Dock Features

These features are in the current product direction but are not complete. They
are not proposed new features.

| ID | Feature | Current state | Acceptance criteria |
|---|---|---|---|
| U02 | Live work-area and display changes | Config geometry is live. A one-second poll repositions on work-area, display, and DPI changes. A real multi-DPI or resolution change is not yet exercised | DPI change, display change, and taskbar hide or restore reposition the dock without a restart. The trigger strip is rechecked |
| U03 | App-list polish | Pinned app management, `show_running_apps`, middle-click launch, and horizontal scrolling exist. Tooltips and a clearer overflow affordance do not | Middle-click launches a second instance for pinned apps. Tooltips show the app name. A long app list remains usable within the configured width |
| U04 | UWP identity | Implemented | `ApplicationFrameHost.exe` windows map to the child `Windows.UI.Core.CoreWindow` process. On 2026-11-11 Settings resolved to `SystemSettings.exe` with the name Settings. The icon uses the frame or path fallback; a visual icon check is still missing |
| U05 | Windows 11 blur verification | Mica and Acrylic code exists and is gated on build 22621. It is not verified on Windows 11 | Mica and Acrylic are exercised on Windows 11 22H2 or newer. The radius behavior in those modes is recorded |
| U06 | Manual robustness coverage | Implemented | A real Explorer restart was tested on 2026-11-11; the replacement taskbar was re-hidden and the tray icon returned. Controlled panic and console-control events restored the taskbar and auto-hide. A fullscreen app still needs a real check with the taskbar visible afterward |
| U07 | Multi-DPI UI coverage | Running UI was exercised at 100% only. Conversions are unit-tested to 200% | The running UI is checked at 125%, 150%, and 200% with correct size and placement |
| U08 | Shutdown latency | Verified | Quit completion is bounded. Three release runs on 2026-11-11 exited in 98 ms, 77 ms, and 77 ms after the quit hotkey |

## 6. Implemented Development Features

These features were proposed after the original plan and are now implemented.
`Implemented` still means at least one manual or environment check is missing.
The installer and hosted workflows need execution evidence before they are called
verified.

### 6.1 Settings UI

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| P01 | Provide a settings window | Verified | A separate, focusable native Win32 window edits the shared config. The dock remains a non-activating tool window. Opened through `--settings` on 2026-11-11; all fields were populated. This deliberately deviates from the original GPUI wording, see D031 |
| P02 | Use draft, Apply, and Close | Verified | Controls hold the draft. Apply validates and saves. Close discards unsaved edits. On 2026-11-11, a draft width of 700 was discarded by Close and the file stayed at 560 |
| P03 | Show validation and errors | Verified | Invalid fields show a message box naming the failure. A failed save leaves the previous file intact and reports the failure. On 2026-11-11, width `abc` produced `width must be a whole number: invalid digit found in string` and the file was unchanged. Inline field errors are not implemented |
| P04 | Show restart notices | Verified | Changing `quit_hotkey` shows a restart-required notice. `hide_native_taskbar` and `offset` are live. On 2026-11-11, a new hotkey saved and showed the restart notice |
| P05 | Share one config owner | Verified | One settings window per user session through a named mutex. A second settings process focuses the existing window. The dock never writes the file. A `--settings` process can coexist with the dock and the watcher reloads its save |
| P06 | Save safely | Verified | Saves use atomic replacement in the same directory. External edits are detected and prompt before overwrite. On 2026-11-11, No preserved the external file and Yes overwrote it. A valid file keeps its comments, key ordering, and unknown keys because known fields are updated in place; an invalid file is not overwritten. Comments and unknown keys inside rewritten `pinned` entries are not preserved |
| P07 | Avoid new CLI behavior | Superseded | `--settings` was added so the window can open without the dock. Existing commands keep their behavior |

The save contract is defined in `docs/CONFIGURATION.md`.

### 6.2 Installer

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| P08 | Choose an install scope | Implemented | Per-user install with `PrivilegesRequired=lowest` and `{autopf}\Dockwind`. No administrator prompt |
| P09 | Use a stable install path | Implemented | The per-user `{autopf}\Dockwind` path and fixed AppId are recorded in D033. Upgrades reuse them |
| P10 | Offer optional autostart | Implemented | The installer does not force autostart. The app and tray expose it through the existing HKCU Run key |
| P11 | Preserve the taskbar on upgrade and uninstall | Verified | Uninstall force-stops `dockwind.exe`, runs `--disable-autostart`, then `--restore-taskbar` before removing files. On 2026-11-11 the uninstall passed while the taskbar was hidden: the app exited, auto-hide returned to 1, the taskbar was visible, and the install directory and shortcuts were removed. The taskbar-visible case also passed |
| P12 | Define the user-data policy | Implemented | Uninstall preserves `%APPDATA%\Dockwind\config.toml` and logs. The installer does not delete user data |
| P13 | Do not guess the packaging tool | Implemented | Inno Setup 6 is selected in D033. The script is in `installer/dockwind.iss` |

The installer was built with Inno Setup 6.7.3 on 2026-11-11. A silent first-run
install with no Dockwind data created the default config and tray icon; the 0.1.0
to 0.2.0 upgrade and uninstall also passed. A clean Windows profile was not
tested. The release workflow installs Inno Setup before building.

### 6.3 Delivery

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| P14 | Add hosted CI | Implemented | GitHub Actions on `windows-latest` runs `scripts/check.ps1` for pushes and pull requests. The gate and installer artifact job passed on 2026-11-11. A manual release dry run built and uploaded the installer without publishing. A tag workflow builds the installer and publishes a release; the tag publish path has not run yet |

## 7. Release Criteria

The current release is ready when all of these are true:

1. The full `scripts/check.ps1` run passes from the repository root. Earlier runs
   are intermediate.
2. Existing config keys, file locations, and CLI behavior still work.
3. Manual checks confirm the native taskbar is restored after normal quit, panic,
   console control, and the recovery command; all passed on 2026-11-11. A real
   Explorer restart was tested. With `reserve_space` and a hidden taskbar, the
   work area stops at the dock edge and is restored on quit.
4. The dock does not take focus from the foreground app.
5. The settings window opens, saves, rejects invalid values, prompts on an
   external edit, and applies live geometry. The restart-only fields are checked.
6. The tray icon appears, its menu opens, and its settings, reload, autostart,
   restore, and quit actions are checked.
7. The installer builds, installs on a clean profile, starts the dock, upgrades
   over the same path, and uninstalls with the taskbar visible.
8. Hosted CI passes on a pull request and the tag workflow publishes an installer.
9. The documented limitations match the code. No limitation is silently removed
   from the docs without evidence.
10. `docs/HANDOFF.md` lists the pending work and the dated evidence that exists.

## 8. Non-Goals

- Replacing Start, the system tray, the notification center, or notification
  badges.
- Supporting Windows 10 blur through the undocumented
  `SetWindowCompositionAttribute` path.
- A plugin system, theme marketplace, or scripting surface.
- A database. Dockwind has no ERD.
- A machine-wide installer, code signing, auto-updater, or delta updates in this
  release.
- Per-field inline validation in the native settings window.
- New features during documentation-only work. `--settings` is part of the
  settings feature, not a standalone expansion.

## 9. Constraints

- Rust 2024 edition.
- GPUI pinned to `=0.2.2`.
- `windows` crate 0.61.
- Windows 10 and 11, x64.
- The settings window uses native Win32 controls. This is recorded in D031.
- The installer uses Inno Setup 6 and a per-user install. This is recorded in
  D033.
- Hosted CI uses GitHub Actions on `windows-latest`. This is recorded in D034.
- Existing TOML keys, file locations, and CLI behavior stay compatible.
  `margin_bottom` remains an alias for `offset`.
- Targeted lints are active: `unsafe_op_in_unsafe_fn = "deny"` and
  `clippy::undocumented_unsafe_blocks = "deny"`. Global `unwrap` and `expect`
  denies are not used; production-only denies live at crate scope.
- No secrets or `.env` files are read or edited without explicit permission.
