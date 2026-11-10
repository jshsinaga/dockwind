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
game was not exercised on this machine. Context menus are implemented in M7;
`menu_open` pins the dock while the menu is open.

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

## D019: Autostart is exposed through the native menu and CLI

Date: 2026-11-10

Right-clicking the dock opens a native Win32 popup with Reload config, an
autostart toggle, and Restore taskbar & Quit. The same autostart operations are
available through `--enable-autostart` and `--disable-autostart`.

Reason: GPUI 0.2.2 has no context-menu API, so the menu is built with
`CreatePopupMenu` and `TrackPopupMenu`. The CLI remains useful for setup and
recovery when the dock is not running.

Trade-off: The autostart value is user-scoped under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Removing it manually is
also possible with Registry Editor.

## D020: Native menu is deferred and pins the dock

Date: 2026-11-10

The right-click handler spawns a foreground task that yields once before calling
`TrackPopupMenu`. The task sets `menu_open` before the blocking call and clears it
afterward; auto-hide treats `menu_open` as `pinned_open`.

Reason: Calling the modal native menu directly inside a GPUI mouse handler
caused a re-entrant `RefCell already borrowed` panic when the menu loop
dispatched queued GPUI tasks. Deferring the call leaves no active app borrow
while the menu runs. The `menu_open` flag keeps the dock visible while the menu
is open.

Trade-off: The menu blocks the GPUI thread for the duration of the popup, but
the deferred task is outside the borrow guard and the auto-hide task can still
observe `menu_open`.

## D021: Canonical documentation map

Date: 2026-11-10

Approved with the project organization plan. `AGENTS.md` is the entry point and
points to canonical documents instead of repeating them:

- `docs/PRD.md`: scope, requirements, status, acceptance criteria.
- `docs/ARCHITECTURE.md`: target modules, ownership, lifetimes, invariants.
- `DESIGN.md`: UI and window patterns.
- `docs/CONFIGURATION.md`: config schema, validation, reload, save contract.
- `docs/engineering.spec.md`: Rust and verification conventions.
- `docs/HANDOFF.md`: current state, pending work, dated evidence.
- `docs/decisions.md`: this record.
- `docs/implementation_plan.md`: historical original plan.
- `README.md`: user-facing setup and behavior.

Dockwind has no database. `docs/ERD.md` does not apply, and the AGENTS pointer
now goes to `docs/CONFIGURATION.md` for the data shape.

Reason: The old set referenced missing specs and repeated current state in several
places. One source of truth per concern makes changes easier to trace and stops
drift between the README, the handoff, and the engineering spec.

Trade-off: The documents must be synchronized after the source integration. Until
then, paths and API names in the docs are target statements. The old missing-spec
guidance in `docs/engineering.spec.md` and `docs/HANDOFF.md` is superseded.

## D022: Feature-grouped target source layout

Date: 2026-11-10

Approved with the project organization plan. The target layout is:

- `src/app/{mod,runtime}.rs`
- `src/config/{mod,model,validation,store,watch}.rs`
- `src/dock/{mod,model,autohide}.rs`
- `src/ui/{mod,dock,app_item,clock,theme}.rs`
- Existing `src/win/*.rs`
- `src/logging.rs`

The agreed integration contract is:

- `config::watch` returns `(ConfigWatcher, UnboundedReceiver<Config>)`.
- `ConfigWatcher::shutdown(&mut self)` stops the watcher thread.
- `AutoHideInput` carries injected time plus the update inputs.
- `WindowInfo` keeps the existing domain fields.
- `DockItem` does not carry an icon. Icons stay in the UI cache.

Reason: The flat `config.rs` and `app_state.rs` mix model, validation, I/O, and
UI-facing grouping. Feature modules give each resource one owner and make the
startup and shutdown path explicit.

Trade-off: The migration started at baseline `482bd7a`. See D027 for the module
landing and D028 for the integrated structure. Native lifecycle fixes are still
pending, so source completion is not final.

## D023: Pin the verified Rust 1.99.1 toolchain

Date: 2026-11-10

`rust-toolchain.toml` pins channel `1.99.1`, the
`x86_64-pc-windows-msvc` target, `rustfmt`, and `clippy`.

Reason: The project verified Rust 1.99.1. An exact release pin makes local and
future CI builds reproducible. This supersedes the `stable` channel choice in
D003 while keeping the MSVC target and components.

Trade-off: Toolchain updates are deliberate. A machine that does not have 1.99.1
installed must download it before the first build.

## D024: Root-independent verification script and deferred lint changes

Date: 2026-11-10

`scripts/check.ps1` is the canonical local gate. It resolves the repository root
from its own path and runs:

1. `cargo fmt --check`
2. `cargo check --all-targets --locked`
3. `cargo clippy --all-targets --locked -- -D warnings`
4. `cargo test --locked`
5. `cargo build --release --locked`
6. Local Markdown file-link checks

It stops at the first failed native command and reports that command and its exit
code. The link check skips external, plugin, and skill URIs, ignores anchors when
checking the file part, and does not create files to satisfy historical examples.

New warnings or denies in `Cargo.toml` were deferred at first. The approved
policy is:

- `[lints.rust] unsafe_op_in_unsafe_fn = "deny"`
- `[lints.clippy] undocumented_unsafe_blocks = "deny"`

The integrator confirmed native coverage on 2026-11-10 and the policy is active.
See D028. Do not add global `unwrap` or `expect` denies. Tests and `build.rs` use
them as permitted fatal errors. Production-only `unwrap` and `expect` are denied
at crate scope separately.

Native SAFETY coverage and `unwrap` or `expect` in tests and `build.rs` have
different constraints. No broad lint allowances are allowed.

Reason: One gate removes command drift and makes link rot visible. Lint changes
can break native code and test code for different reasons, so they need one
coordinated change.

Trade-off: The gate takes longer than a single `cargo test`. Hosted CI is deferred
until a host is selected. The two targeted lints are active.

## D025: Settings UI and installer stay future

Date: 2026-11-10

The settings window and installer are proposed features, not current release
scope. Their requirements and contracts are recorded in `docs/PRD.md`,
`docs/CONFIGURATION.md`, and `DESIGN.md`.

- Settings: one shared config owner, a focusable separate GPUI window, drafts,
  Apply and Cancel, validation and errors, restart notices, atomic save, watcher
  feedback, external edit conflicts, and preservation of user content.
- Installer: user-scope recommendation, a stable path chosen by a later decision,
  optional autostart, upgrade and shutdown taskbar restore, an uninstall
  user-data policy, and no packaging tool guess.

Reason: The current release needs a reliable dock and taskbar recovery. A settings
UI and installer add write, conflict, upgrade, and uninstall failure paths. They
need their own design and verification.

Trade-off: Users edit `config.toml` by hand and install by copying or building.
`reserve_space` and `clock.locale` remain accepted but ineffective until their
features exist.

## D026: The original implementation plan is historical

Date: 2026-11-10

`docs/implementation_plan.md` keeps its body and gains a status note. It remains
useful for the original rationale, milestones, and manual test list. It is not
current scope.

`docs/PRD.md`, `docs/ARCHITECTURE.md`, `docs/CONFIGURATION.md`, and
`docs/HANDOFF.md` supersede it for current requirements, structure, config, and
state.

Reason: Deleting the plan would lose the original intent and the M0 gate record.
Rewriting it would blur what was planned and what was later decided.

Trade-off: Some details in the plan are stale, including old file paths and
milestone status. Readers must follow the status note and not copy its paths.

## D027: Config, dock, and app modules land in isolation

Date: 2026-11-10

The config and dock parts of D022 landed in source:

- `src/config/{mod,model,validation,store,watch}.rs`
- `src/dock/{mod,model,autohide}.rs`

`config::watch` returns `(ConfigWatcher, UnboundedReceiver<Config>)`.
`ConfigWatcher::shutdown` is idempotent, stops the worker, and joins it. The
watcher observes the parent directory, filters to the config path, and survives
repeated atomic replacement. `ConfigError::WatcherPanicked` is additive.
`AutoHideInput` carries injected time and booleans. `WindowInfo` keeps the old
domain fields. `DockItem` has no icon.

The config module reports 22 isolated tests passing. This is not a full repository
gate. `src/app/*` and `src/ui/theme.rs` also landed later on 2026-11-10. See D028
for the integrated structure and current native review stage.

Reason: The split gives config and dock clear owners and removes the mixed I/O,
validation, and UI model from the flat files.

Trade-off: The docs must distinguish isolated module tests from full integration.
Do not mark the target layout or its API as fully verified until
`scripts/check.ps1` passes.

## D028: Integrated structure, active lints, and pending native fixes (superseded by D029)

Date: 2026-11-10

The target layout is integrated:

- `src/main.rs` dispatches to `src/app/runtime.rs`.
- `src/app/mod.rs` owns `AppController`; the runtime keeps a strong reference
  outside the dock view for the GPUI event loop.
- `src/ui/mod.rs` creates a `DockRoot` that owns the `DockView` after successful
  window setup.
- `src/ui/theme.rs` is in place.
- `src/win/worker.rs` owns the manual-reset stop event and
  `MsgWaitForMultipleObjectsEx` wake path shared by the event, taskbar, and hotkey
  workers. Shutdown does not depend on `PostThreadMessageW`.
- The taskbar state mutex guards the ownership flag, visibility records, restore,
  and `TaskbarCreated` re-hide.
- Console handler registration is required before the taskbar is hidden.
- The logger is the last runtime field to drop.

Targeted lints are active in `Cargo.toml`:

- `[lints.rust] unsafe_op_in_unsafe_fn = "deny"`
- `[lints.clippy] undocumented_unsafe_blocks = "deny"`

There is no global `unwrap` or `expect` deny. Production-only
`clippy::unwrap_used` and `clippy::expect_used` are denied at crate scope with
`cfg(not(test))` in `src/main.rs`.

Intermediate evidence on 2026-11-10:

- Full `scripts/check.ps1` passed before the native re-review: fmt, check, clippy,
  59 tests, release build, and link checks.
- The baseline `proc-macro-error2` v2.0.1 warning remains.
- Debug startup smoke ran for 4 seconds on Windows 10 build 19045, 1920x1080,
  100% DPI, `hide_native_taskbar = false`. Logs showed config load, hotkey
  registration, placement, and transparent styles. The process was force-killed.

This is not final completion. Native re-review found four lifecycle issues:

1. The stop-aware wait can still block in `GetMessageW` after a sent-only wake.
2. The emergency force-show path is missing the ownership latch.
3. A failed `SetEvent` stop signal still leads to a join.
4. Race tests duplicate policy checks.

Source fixes are transferring to the Escalation profile. Do not mark native or
source completion. Do not treat a gate run now as final.

Not manually verified on the integrated code: normal quit, taskbar hide and
restore, Explorer restart, panic, console control, `--restore-taskbar`, Windows
11, Mica or Acrylic, and a real multi-DPI desktop.

Reason: The structure and targeted lints are ready, and the intermediate gate
gives useful evidence. The remaining native lifecycle issues affect shutdown and
recovery, so they block final completion.

Trade-off: The project has a working integrated structure but no final gate. One
full gate runs after the critical fixes and review.

## D029: Native lifecycle contract reviewed and cleared

Date: 2026-11-10

The four native lifecycle findings are resolved and independently cleared:

1. `worker::next_message` uses a stop-aware wait and one `PeekMessageW`
   consumption per wake. It does not block in `GetMessageW` after a sent-only
   wake. `WM_QUIT` returns `None` before dispatch.
2. The wait polls the manual-reset stop event with `WaitForSingleObject(0)` before
   `MsgWaitForMultipleObjectsEx`, so a busy queue cannot starve cancellation.
3. Shutdown signals a live worker before consuming the stop or join handle. A
   signal failure returns the original error and retains both handles. A completed
   worker joins without signaling. A joined panic or error closes the event
   without aborting.
4. `TaskbarOwnership` uses the state mutex plus an irreversible recovered
   `AtomicBool` latch. Recovery sets the latch before showing windows and does not
   take the state mutex. Every hide checks the latch before and after each native
   effect. An in-flight hide that completes after recovery compensates by
   force-showing its recorded handles.

The runtime keeps native worker owners available through error reporting and
retries. The logger still drops last. Terminal abort is limited to a drop-time
cancellation failure while the worker is still live; recovery and a synchronous
stderr diagnostic run first.

Targeted tests use production methods with fake taskbar effects. They cover both
normal mutex interleavings, a late callback, recovery between hides, and recovery
during an in-flight effect. A message-only window test covers sent-only wake and
stop, `WM_QUIT`, and a busy queue. An injected cancellation failure at the signal
operation verifies ownership retention and retry.

Independent review is cleared. The final full gate passed on 2026-11-10:
`scripts/check.ps1` ran fmt, check, clippy with `-D warnings`, 66 tests, the
release build, the 16-case link parser self-test, and 27 Markdown links in 11
files. Manual desktop cases remain unverified: normal quit, taskbar hide and
restore, real Explorer restart, panic, console control, `--restore-taskbar`, GUI
smoke after the fixes, Windows 11, Mica or Acrylic, and real multi-DPI.

Reason: The review findings affected cancellation, emergency recovery, and
shutdown ownership. The fixes make the failure paths explicit and testable.

Trade-off: The source is review-cleared and the full gate passed, but manual
desktop behavior is not verified until dated checks exist.

## D030: Show the dock after the first GPUI frame

Date: 2026-11-10

A release run exposed an empty white rectangle at the dock position. The window
was created with `show: true`, so Windows could display the native surface before
GPUI presented its first frame. Once the dock repainted, it rendered correctly.
The hidden auto-hide state was also transparent when the first frame completed.

Decision: create the dock window with `show: false`. After `cx.open_window`
returns, GPUI has drawn once. Show the window with `SW_SHOWNOACTIVATE` and verify
`IsWindowVisible`. A show failure aborts startup instead of leaving an invisible
dock.

Trade-off: startup now performs one explicit native show call. The window is not
visible until the first frame is ready, which removes the white surface but adds
a small delay before the dock can appear.

Evidence: the 2026-11-10 release run logged `dock window shown after first frame`.
The hidden state was invisible and the shown state rendered icons and the clock.

Superseded by D031 and D033 on 2026-11-11. The settings window and installer are
now implemented.

## D031: Settings window uses native Win32 controls

Date: 2026-11-11

The settings window is a native Win32 window on a dedicated thread. It is not a
second GPUI window. Standard edit, button, combo box, list box, and dialog
controls hold the draft. Apply validates and writes the config atomically. The
existing file watcher applies the result to the dock.

Reason: Standard controls give a reliable form without building a form toolkit on
top of GPUI. A separate thread isolates a blocked file dialog or a settings
failure from the dock event loop. The watcher already provides the live-apply
path, so the window does not need GPUI state.

Trade-off: The settings window does not share the dock's visual language. Field
errors are message boxes, not inline text. The window is more native code than a
GPUI form would be. This deviates from the original GPUI wording in PRD P01.

## D032: Config geometry is live and orientation is top or bottom

Date: 2026-11-11

`appearance.width`, `height`, `offset`, and `orientation` are live. A config
reload recomputes the primary work area rect and calls `SetWindowPos` with
physical pixels. The auto-hide trigger moves to the selected edge and the hide
animation direction follows it. `margin_bottom` remains an alias for `offset`.

Reason: A settings window that needs a restart for size or placement is a poor
configuration surface. The live path also supplies the geometry primitive needed
by later DPI and display work.

Trade-off: Automatic `WM_DPICHANGED`, display change, and taskbar work-area
changes are still not handled. Those events need their own message source and
tests.

## D033: Inno Setup per-user installer

Date: 2026-11-11

The installer uses Inno Setup 6, a fixed AppId, and a per-user install under
`{autopf}\Dockwind` with `PrivilegesRequired=lowest`. It creates a Start Menu
shortcut and an optional desktop shortcut. It does not force autostart. Uninstall
runs `--disable-autostart` and `--restore-taskbar` before removing files. Config
and logs are preserved.

Reason: A per-user install needs no administrator prompt and matches the HKCU
autostart model. Inno Setup produces one readable EXE and is available on the
Windows CI runner.

Trade-off: The installer and binary are unsigned. SmartScreen can warn. A
machine-wide install, code signing, and auto-update are out of scope for this
release. Inno Setup is an external build tool and was not installed on the
development machine, so the installer was not built locally.

## D034: GitHub Actions CI and tag releases

Date: 2026-11-11

GitHub Actions on `windows-latest` runs `scripts/check.ps1` for pushes to `main`,
pull requests, and manual dispatch. A `vX.Y.Z` tag workflow verifies the tag
against `Cargo.toml`, builds the release, installs Inno Setup, builds the
installer, uploads the artifact, and publishes a GitHub Release.

Reason: The repository already lives on GitHub. The local gate is a PowerShell
script, so the hosted job can reuse the exact same commands. A tag check prevents
an installer whose version does not match the source.

Trade-off: The workflows depend on the hosted runner, Chocolatey, and network
access to install Inno Setup. They have not run in the hosted environment yet.

## D035: Tray icon runs on its own worker thread

Date: 2026-11-11

The tray icon uses `Shell_NotifyIconW` on a hidden window in a dedicated
`dockwind-tray` thread. The window procedure handles the callback message,
`TaskbarCreated`, and the tray menu. Menu actions are sent to `AppController`
through an unbounded channel. A missing tray icon is logged and does not block
the dock.

Reason: The tray needs its own window and message loop. Keeping it off the GPUI
thread avoids blocking the dock on a modal menu. The shared stop-aware message
reader gives the thread the same cancellation contract as the other workers.

Trade-off: Another native thread and hidden window. The tray icon is a second
surface that can fail independently. The failure is non-fatal by design.

## D036: Add a --settings CLI entry point

Date: 2026-11-11

`dockwind --settings` opens the settings window and keeps the process alive until
the window closes. It runs before the single-instance check, like
`--restore-taskbar`, so it can be used while the dock is running. It focuses an
existing settings window in the same process.

Reason: The dock may be hidden, the tray icon may be missing, and a user may want
to open settings without a dock interaction. A CLI entry also makes the window
testable in isolation.

Trade-off: This is a new CLI flag, so PRD P07 is superseded. A second process can
open a second settings window. Coordination between settings processes is future
work.

## D037: Poll the primary monitor once per second

Date: 2026-11-11

The dock reads the primary monitor work area, display bounds, and effective DPI
once per second from the existing UI tick. A changed snapshot recomputes the dock
rect and trigger and calls `SetWindowPos`. Config geometry changes remain event
driven through the config watcher.

Reason: A message-only window plus `WM_SETTINGCHANGE`, `WM_DISPLAYCHANGE`, and
`WM_DPICHANGED` would add another native window and message path. The dock already
ticks for cursor and auto-hide state, so a one-second check covers taskbar,
resolution, and DPI changes with one code path and no new window.

Trade-off: A change can take up to one second to apply. The check calls
`GetMonitorInfoW` and `GetDpiForMonitor` every second, even when nothing changed.
A real multi-DPI or resolution change is not manually exercised yet.
