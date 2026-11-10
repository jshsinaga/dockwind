# Decisions

Recorded during implementation. Each entry states the decision, the reason, and
the trade-off.

## D001: Pin GPUI 0.2.2 from crates.io

Date: 2026-11-10

Use `gpui = { version = "=0.2.2", default-features = false, features = ["font-kit"] }`
instead of a Git revision or a moving branch.

Reason: GPUI 0.2.2 has native Windows support, exposes `raw-window-handle` 0.6,
and provides the window styles needed by M0. An exact crate pin is reproducible
and avoids the larger Zed repository checkout.

Trade-off: GPUI is pre-1.0. Upgrades require re-checking the APIs used here.
Disabling default features turns off GPUI's own Windows manifest, Wayland, and
X11 support; Dockwind only targets Windows.

## D002: Dockwind owns the Windows manifest

Date: 2026-11-10

Disable GPUI's `windows-manifest` feature and embed `assets/dockwind.manifest`
through `build.rs` and `assets/dockwind.rc`.

Reason: GPUI already embeds an `RT_MANIFEST` resource with PerMonitorV2. Adding a
second manifest would conflict. Owning the manifest lets Dockwind add
`longPathAware`, the Dockwind assembly identity, and the Windows 10/11
supported-OS entry in one place.

The generated executable was checked with `mt.exe`; the resource contains
PerMonitorV2, `longPathAware`, Common Controls v6, and the Dockwind identity.

Trade-off: If GPUI later needs manifest entries of its own, Dockwind's manifest
must be kept in sync.

## D003: Pin the stable MSVC toolchain

Date: 2026-11-10

`rust-toolchain.toml` selects the `stable` channel, the
`x86_64-pc-windows-msvc` target, `rustfmt`, and `clippy`.

Reason: The plan requires a pinned stable toolchain and the MSVC target. The
current verified version is Rust 1.99.1.

Trade-off: The channel is stable rather than an exact patch release, so a future
toolchain update can change compiler behavior. The verified version is recorded
in `docs/spike-m0.md`.

## D004: Invalid config is explicit, logged degradation

Date: 2026-11-10

Invalid config values are replaced with defaults and reported as warnings.
A corrupt file at startup uses defaults with a warning. A corrupt file during
hot reload is rejected and the last valid config is kept.

Reason: The implementation plan requires the application to keep running with a
warning for invalid config. This is an explicit, tested exception to the general
ban on silent fallbacks; it is never silent.

Trade-off: The invalid file remains on disk until the user fixes it. Dockwind
does not overwrite the user's file on a parse failure.

## D005: Non-activating dock window

Date: 2026-11-10

Use `WS_EX_NOACTIVATE` on the dock and restore the previous foreground window
after creation.

Reason: `WindowOptions.focus = false` alone did not prevent the dock from
becoming the foreground window. M0 showed that the dock took focus, then showed
that `WS_EX_NOACTIVATE` plus `SetForegroundWindow(previous)` keeps the target
application focused. Mouse clicks still reach the dock.

M3 hardened the restore path: it only runs when the dock is actually the
foreground window, attaches to the foreground thread when needed, and logs a
warning instead of aborting startup if Windows still denies the request.

Trade-off: The dock has no keyboard focus. Keyboard-driven dock actions and
context menus need their own M5/M6 verification.

## D006: The `dockwind` crate name is available

Date: 2026-11-10

`cargo search dockwind` returned no packages, and the crates.io API returns 404
for `dockwind`.

Reason: The plan asks for a name availability check before publishing.

Trade-off: Availability now does not reserve the name. No publish action was
taken.

## D007: Config watcher with a 200 ms debounce

Date: 2026-11-10

Use `notify::recommended_watcher` on the config file and drain events for 200 ms
before reloading.

Reason: Editors often emit several write events for one save. The debounce
avoids repeated parses and reduces the chance of reading a partially written
file.

Trade-off: The watcher forwards the new `Config` to the GPUI view over an
unbounded channel. M2 consumes it in `DockView`, which updates the theme and
applies a changed blur mode through the stored window handle.

## D008: Position against the monitor work area

Date: 2026-11-10

Use `MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY)`,
`GetMonitorInfoW`, and `GetDpiForMonitor(MDT_EFFECTIVE_DPI)` to build a
`MonitorInfo` for the primary display. `dock_rect` is a pure function over
logical pixels.

Reason: `PlatformDisplay::bounds()` gives the full monitor rectangle but not the
work area. Using `rcWork` keeps the dock above the visible taskbar until M6
hides it. Converting physical pixels to logical pixels with the effective DPI
keeps GPUI's window bounds correct.

Trade-off: The UI is only placed once at startup. A display change or taskbar
work-area change requires a restart until M5/M6 add recalculation. The running
UI was verified at 100% DPI; 125%, 150%, and 200% are unit-tested conversions
only on this machine.

## D009: Apply blur and corner changes through the dock window handle

Date: 2026-11-10

Capture `Window::window_handle()` during window creation and pass it to
`DockView`. When the watcher delivers a config with a different blur mode, use
`AsyncApp::update_window` to call `win::dwm::apply_backdrop`.

Reason: The `DockView` itself has no direct handle to its platform window during
render, and blur is a DWM property rather than a GPUI style. The stored GPUI
handle lets the update run on the UI thread with the real `Window`.

Trade-off: A closed window makes the update fail; the failure is logged and the
config update for the theme is still retained. DWM backdrop remains gated on
Windows 11 build 22621 and is not validated on this Windows 10 machine.

## D010: Clock ticks on real time boundaries

Date: 2026-11-10

`ClockView` computes the delay to the next minute, or the next second when the
time format contains `%S`, `%T`, or `%r`. It does not use a fixed polling
interval.

Reason: The plan requires the clock to change at the minute boundary without
wasting CPU on a fixed short interval. A live format change is applied
immediately, and the timer is restarted when the precision changes.

Invalid time and date formats are rejected during config sanitization using
`chrono::format::StrftimeItems` and replaced with defaults with a warning.

Trade-off: A format change that keeps the same precision does not restart the
timer. The new text is rendered immediately, and the next tick stays aligned to
the existing boundary.

## D011: App-list refresh is event-triggered full enumeration

Date: 2026-11-10

Install two `SetWinEventHook` ranges on a dedicated message-loop thread. Windows
events are coalesced with a 120 ms debounce, then the dock re-enumerates all
top-level windows and rebuilds the app list.

Reason: Full enumeration is simpler and more reliable than an incremental
`apply_event` while window ownership, cloaking, titles, and process paths all
affect grouping. Icons are cached by executable path, so repeat enumerations
are cheap after the first one.

Trade-off: Each refresh walks all top-level windows. The hook filters to window
show/hide/destroy/name/foreground/minimize events, and the debounce prevents
event storms. If the app list grows very large, this can move to incremental
updates.

## D012: Icon extraction order and cache

Date: 2026-11-10

Extract icons in this order: `WM_GETICON(ICON_BIG)`, then
`GetClassLongPtrW(GCLP_HICON)`, then `SHGetFileInfoW` for the executable. Convert
`HICON` through `GetIconInfo` and a 32-bit DIB into BGRA `RenderImage` data, and
cache the result by executable path.

Reason: The first two sources track the live window icon; the executable icon is
the fallback for pinned apps that are not running. The cache avoids repeating
the GDI conversion on every window event.

Trade-off: The first icon source wins for a grouped executable, so windows with
per-document icons share one icon. The cache has no eviction; the number of
running executables is expected to stay small.

## D013: Click behavior and foreground activation

Date: 2026-11-10

A left click on a running app activates its most recent non-minimized window, or
minimizes it when that window is already foreground. A click on a pinned app
that is not running launches it with `ShellExecuteW`. Foreground changes retry
through `AttachThreadInput` when Windows denies `SetForegroundWindow`.

Reason: The dock has `WS_EX_NOACTIVATE`, so the click does not give Dockwind
foreground permission. The attach retry is the documented workaround for the
foreground lock.

Trade-off: Middle-click launch and per-window selection are not implemented yet.
UWP apps behind `ApplicationFrameHost.exe` are skipped.

## D014: Auto-hide uses a state machine and click-through

Date: 2026-11-10

`AutoHide` is a pure state machine over injected monotonic time. The dock polls
the cursor every 100 ms while hidden and every 16 ms while visible or animating.
Progress drives the bar's opacity and vertical offset. When progress reaches
zero, Dockwind sets `WS_EX_TRANSPARENT` on the dock so the invisible topmost
window cannot intercept clicks.

Reason: Keeping the window at the work-area edge avoids a second window position
model, while click-through makes the hidden state functionally invisible. The
state machine is testable without GPUI or Win32.

Trade-off: During the fade there is still a live window at the bottom edge. The
fullscreen check is unit-tested through `force_hidden`, but a real fullscreen
game was not exercised on this machine. Context menus are not implemented, so
the `pinned_open` input is always false.

## D015: Auto-hide trigger is the work-area edge

Date: 2026-11-10

The trigger is a 2 px strip at the bottom of the monitor's work area, not the
screen edge.

Reason: The taskbar is still visible until M6. The work-area edge is the
bottom-most reachable desktop edge and remains correct when the taskbar is
hidden later.

Trade-off: With a bottom taskbar, the pointer must be just above the taskbar,
not on the taskbar itself. M6 will expand the work area when it hides the native
taskbar.

## D016: Native taskbar safety and recovery

Date: 2026-11-10

Hide `Shell_TrayWnd` and `Shell_SecondaryTrayWnd` with `ShowWindow(SW_HIDE)`,
record whether each was visible, and restore through a `TaskbarGuard` drop. Also
install a panic hook, a console control handler, and a hidden top-level window
that handles `WM_ENDSESSION` and the `TaskbarCreated` broadcast. The standalone
`--restore-taskbar` path forces the taskbar visible even if no state exists.

Reason: A crash or Explorer restart must not leave the user without a taskbar.
The watcher re-hides a restarted taskbar while Dockwind is running; the force
restore handles a kill that bypassed the guard.

Trade-off: Hiding is performed with `ShowWindow`, not by stopping Explorer, so
Start, the system tray, and notification center remain unavailable while hidden.
`reserve_space` and AppBar reservation are not implemented. The Explorer restart
case was tested by broadcasting `TaskbarCreated`, not by restarting Explorer.

## D017: Single instance and global quit hotkey

Date: 2026-11-10

Use a named mutex (`Local\Dockwind.SingleInstance`) and a dedicated hotkey
thread. The hotkey thread registers `quit_hotkey`, waits for `WM_HOTKEY`, then
sends a quit event consumed by the GPUI view.

Reason: Two docks would fight over the taskbar and config. The hotkey runs on a
thread with its own message queue because `RegisterHotKey` delivers
`WM_HOTKEY` to the registering thread. `--restore-taskbar` bypasses the mutex so
it can recover a running or crashed instance.

Trade-off: If the hotkey is unavailable and `hide_native_taskbar = true`, startup
fails before hiding rather than running without a safe quit path. If taskbar
hiding is off, a hotkey failure is only logged.

## D018: Taskbar settings are startup-only

Date: 2026-11-10

`hide_native_taskbar` and `quit_hotkey` are read during startup. Hot reload
updates appearance, clock, pinned apps, and auto-hide, but not the taskbar or
hotkey.

Reason: Registering/unregistering global hotkeys and installing/removing the
taskbar guard mid-run adds failure paths without being required by M6.

Trade-off: Changing either setting requires restarting Dockwind. This is noted
in the README.
