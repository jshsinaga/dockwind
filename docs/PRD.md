# Dockwind Product Requirements

Status: active. This document defines what Dockwind builds and why. It owns scope
and acceptance criteria. It does not own architecture or configuration details.

Last updated: 2026-11-10
Current code: integrated and review-cleared
Current stage: final full gate passed on 2026-11-10. Manual desktop cases remain
unverified.
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
| R08 | Keep startup-only settings explicit | Verified | `hide_native_taskbar`, `quit_hotkey`, and `margin_bottom` apply only at startup. The docs state it. A future settings UI must show a restart notice |
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
| R21 | Hide the native taskbar only when configured | Verified | `hide_native_taskbar` defaults to false. The hide path is not entered by default |
| R22 | Restore the taskbar on normal quit | Implemented | The `TaskbarGuard` restores the saved-visible taskbar windows. Historical normal-quit evidence exists at `482bd7a`; the integrated runtime has not been manually quit yet |
| R23 | Restore the taskbar on panic and console control events | Implemented | The panic hook and console handler use the lock-free force-show path with the irreversible recovered latch. No dated manual panic or console restore test exists for the integrated code |
| R24 | Restore the taskbar on session end and Explorer restart | Implemented | `WM_ENDSESSION` and a simulated `TaskbarCreated` broadcast exist. A real Explorer restart is not yet exercised |
| R25 | Provide a standalone recovery command | Implemented | `dockwind --restore-taskbar` force-shows all taskbar windows and runs before the single-instance check. Historical recovery evidence exists at `482bd7a`; the integrated binary has not been re-tested |
| R26 | Prevent two dock instances | Verified | A named mutex rejects a second instance. The restore command bypasses the mutex |
| R27 | Provide a global quit hotkey | Implemented | The default `Ctrl+Alt+Shift+Q` registers at startup. Registration is observed in the debug smoke; the quit action has not been manually re-tested on the integrated code |

### 4.6 Integrations

| ID | Requirement | Status | Acceptance criteria |
|---|---|---|---|
| R28 | Provide a native right-click menu | Verified | The menu opens, Escape dismisses it, config reload works, autostart toggles, and Restore taskbar and Quit exits |
| R29 | Support per-user autostart | Verified | The menu and `--enable-autostart` / `--disable-autostart` create and remove the HKCU Run value |
| R30 | Keep the existing CLI contract | Implemented | The binary supports `--restore-taskbar`, `--enable-autostart`, and `--disable-autostart`. No new flags are required. The integrated recovery CLI has not been manually re-tested |
| R31 | Keep file locations stable | Verified | Config stays at `%APPDATA%\Dockwind\config.toml`. Logs stay at `%LOCALAPPDATA%\Dockwind\logs` |

## 5. Accepted Unfinished Dock Features

These features are in the current product direction but are not complete. They
are not proposed new features.

| ID | Feature | Current state | Acceptance criteria |
|---|---|---|---|
| U01 | AppBar `reserve_space` | `reserve_space` exists in config but is unused. `SHAppBarMessage` is not implemented | With `reserve_space = true` and auto-hide off, maximized windows do not cover the dock. `ABM_REMOVE` runs on shutdown |
| U02 | Live work-area and display changes | Placement is computed once at startup. Hiding the taskbar does not update `rcWork` | DPI change, display change, and taskbar hide or restore reposition the dock without a restart. The trigger strip is rechecked |
| U03 | App-list polish | No middle-click launch, tooltips, scrolling, or overflow | Middle-click launches a second instance for pinned apps. Tooltips show the app name. A long app list remains usable within the fixed window |
| U04 | UWP identity | `ApplicationFrameHost.exe` windows are skipped | A UWP app shows its real identity and icon, or the limitation is documented with a supported alternative |
| U05 | Windows 11 blur verification | Mica and Acrylic code exists and is gated on build 22621. It is not verified on Windows 11 | Mica and Acrylic are exercised on Windows 11 22H2 or newer. The radius behavior in those modes is recorded |
| U06 | Manual robustness coverage | Some Win32 paths are unit-tested or simulated, not exercised for real | A real Explorer restart, panic restore, console-control restore, and fullscreen app are tested with the taskbar visible again afterward |
| U07 | Multi-DPI UI coverage | Running UI was exercised at 100% only. Conversions are unit-tested to 200% | The running UI is checked at 125%, 150%, and 200% with correct size and placement |
| U08 | Shutdown latency | Quit works, but the process can exit several seconds after the request | Shutdown completes in a bounded time. Teardown is profiled if the delay remains |

## 6. Proposed New Features

These features are future work. They are not approved for the current release and
must not be implemented without a new decision.

### 6.1 Settings UI

| ID | Requirement | Acceptance criteria |
|---|---|---|
| P01 | Provide a settings window | A separate, focusable GPUI window edits the shared config. The dock remains a non-activating tool window |
| P02 | Use draft, Apply, and Cancel | Edits stay in a draft until Apply. Cancel discards. Apply validates before saving |
| P03 | Show validation and errors | Invalid fields show inline errors. A failed save leaves the previous file intact and reports the failure |
| P04 | Show restart notices | Changing `hide_native_taskbar`, `quit_hotkey`, or `margin_bottom` shows a restart-required notice |
| P05 | Share one config owner | The settings window and the dock do not write the file concurrently. One owner serializes writes and reloads |
| P06 | Save safely | Saves use atomic replacement in the same directory. External edits are detected and never silently overwritten. User content is preserved where the format allows |
| P07 | Avoid new CLI behavior | The settings window does not add CLI flags. Existing commands keep their behavior |

The save contract is defined in `docs/CONFIGURATION.md`.

### 6.2 Installer

| ID | Requirement | Acceptance criteria |
|---|---|---|
| P08 | Choose an install scope | User-scope installation is the recommendation. A machine-wide scope needs a separate decision |
| P09 | Use a stable install path | The path is recorded in a decision before implementation. Upgrades use the same path |
| P10 | Offer optional autostart | The installer can enable autostart through the existing HKCU Run key, but does not force it |
| P11 | Preserve the taskbar on upgrade and uninstall | Upgrade and uninstall stop Dockwind through a path that restores the taskbar. A killed installer does not leave the taskbar hidden |
| P12 | Define the user-data policy | Uninstall states whether config and logs are removed, kept, or offered as a choice. The default preserves user content |
| P13 | Do not guess the packaging tool | The packaging format and tool are selected in a decision before implementation |

### 6.3 Delivery

| ID | Requirement | Acceptance criteria |
|---|---|---|
| P14 | Add hosted CI | Hosting is selected first. CI runs the same gates as `scripts/check.ps1` on Windows |

## 7. Release Criteria

The current release is ready when all of these are true:

1. Independent review is cleared. This is complete.
2. The final `scripts/check.ps1` run passes from the repository root. Earlier
   runs are intermediate.
3. Existing config keys, file locations, and CLI behavior still work.
4. Manual checks confirm the native taskbar is restored after normal quit, panic,
   console control, and the recovery command. A real Explorer restart is checked
   or explicitly recorded as unverified.
5. The dock does not take focus from the foreground app.
6. The documented limitations match the code. No limitation is silently removed
   from the docs without evidence.
7. `docs/HANDOFF.md` lists the pending work and the dated evidence that exists.

## 8. Non-Goals

- Replacing Start, the system tray, the notification center, or notification
  badges.
- Supporting Windows 10 blur through the undocumented
  `SetWindowCompositionAttribute` path.
- A plugin system, theme marketplace, or scripting surface.
- A database. Dockwind has no ERD.
- A settings UI or installer in the current release.
- New CLI flags or product features during documentation work.

## 9. Constraints

- Rust 2024 edition.
- GPUI pinned to `=0.2.2`.
- `windows` crate 0.61.
- Windows 10 and 11, x64.
- Existing TOML keys, file locations, and CLI behavior stay compatible.
- Targeted lints are active: `unsafe_op_in_unsafe_fn = "deny"` and
  `clippy::undocumented_unsafe_blocks = "deny"`. Global `unwrap` and `expect`
  denies are not used; production-only denies live at crate scope.
- No secrets or `.env` files are read or edited without explicit permission.
