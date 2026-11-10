# Dockwind Architecture

Status: active. The target layout is integrated and the native lifecycle contract
is stable. Settings, tray, live config geometry, and release delivery are
integrated. The final full gate passed on 2026-11-10 after those additions.

Last updated: 2026-11-11
Current code: review cleared, final full gate passed
Historical baseline: `482bd7a` (pre-migration layout)

Read `docs/PRD.md` for scope and `docs/HANDOFF.md` for current state and evidence.
Dockwind has no database, so there is no ERD.

## 1. Design Principles

1. Keep pure logic independent of GPUI and Win32 so it can be tested.
2. Keep all `unsafe` and raw handle work under `src/win/`.
3. Keep one owner for each long-lived resource.
4. Keep UI work on the GPUI thread. Send events to it through channels.
5. Fail loudly. Invalid config and unsupported blur are the only logged
   degradations, and both are defined in `docs/CONFIGURATION.md`.
6. Make shutdown and taskbar recovery explicit. Do not rely on process exit alone.

## 2. Target Module Map

The target layout groups by feature. Do not create empty feature directories. A
module exists only when it has code.

| Path | Responsibility | Owns |
|---|---|---|
| `src/main.rs` | CLI dispatch and exit codes | Nothing long-lived |
| `src/app/mod.rs` | `AppController`: active config, app items, config path, commands, event subscriptions | Application state |
| `src/app/runtime.rs` | Startup order, channel wiring, guard and worker ownership, strong controller retention, shutdown | Process-level resources |
| `src/config/mod.rs` | Public config exports used by consumers: types, path, color parser, watcher | Re-exports |
| `src/config/model.rs` | Serde types, defaults, pinned app model | Config data shape |
| `src/config/validation.rs` | Sanitization, color parsing, clock format checks | Validation rules |
| `src/config/store.rs` | Default path, load, create, atomic save, `ConfigError` | Config file I/O |
| `src/config/watch.rs` | Absolute path resolution, parent-directory watch, debounce, reload, shutdown | Watcher thread and receiver |
| `src/dock/mod.rs` | Dock domain boundary and coordination | Dock domain exports |
| `src/dock/model.rs` | `DockItem`, pinned and running merge, grouping | Pure dock model |
| `src/dock/autohide.rs` | Auto-hide state machine | Pure state machine |
| `src/ui/mod.rs` | Window creation, placement, styles, backdrop, `DockRoot` ownership of the created view | GPUI window setup |
| `src/ui/dock.rs` | Dock view, controller observation, polling, menu, clicks | Dock UI state |
| `src/ui/app_item.rs` | App icon and running or active indicator | Item rendering |
| `src/ui/clock.rs` | Clock and date formatting, boundary timer | Clock entity |
| `src/ui/theme.rs` | Color, contrast, radius, padding mapping | Derived theme |
| `src/win/worker.rs` | Manual-reset stop event and `MsgWaitForMultipleObjectsEx` cancellation shared by the event, taskbar, hotkey, and tray workers | Worker cancellation primitive |
| `src/win/appbar.rs` | AppBar registration, reserved rect, callback window thread, remove on release | AppBar reservation |
| `src/win/settings.rs` | Native Win32 settings window thread, controls, validation, comment-preserving atomic save, external-edit prompt, session-wide named mutex | Settings window lifetime |
| `src/win/tray.rs` | Tray icon worker, menu, `TaskbarCreated` recovery, two-second visible-taskbar icon check | Tray icon and menu |
| `src/win/*.rs` | Win32 wrappers for windows, DWM, monitors, icons, events, taskbar, appbar, hotkey, menu, settings, tray, autostart, single instance, cursor, launch | OS resources |
| `src/logging.rs` | Tracing setup and daily log file | Log worker guard |

`src/win/` stays flat. It is a platform boundary, not a feature layer.

At 2026-11-10, the target layout is integrated. `src/main.rs` dispatches to
`src/app/runtime.rs`. The native lifecycle contract is stable and independently
reviewed. The final full gate passed on 2026-11-10.

## 3. Current to Target Mapping

| Historical path at `482bd7a` | Target path | Note |
|---|---|---|
| `src/main.rs` | `src/main.rs` plus `src/app/runtime.rs` | Integrated. `main` dispatches to the runtime |
| `src/config.rs` | `src/config/{mod,model,validation,store,watch}.rs` | Integrated |
| `src/app_state.rs` | `src/dock/model.rs` | Integrated |
| `src/autohide.rs` | `src/dock/autohide.rs` | Integrated |
| `src/theme.rs` | `src/ui/theme.rs` | Integrated |
| `src/ui/*` | `src/ui/*` | Integrated. `DockView` observes `AppController` |
| `src/win/*` | `src/win/*` | Integrated. `worker.rs` added |
| `src/logging.rs` | `src/logging.rs` | Unchanged |

`src/dock/mod.rs` re-exports the pure dock types and `src/ui/dock.rs` owns
`DockView`. Do not document an API that has not landed.

## 4. Integration Contract Status

The contract is integrated. The native lifecycle contract is stable and
review-cleared. The final full gate passed on 2026-11-10.

| API | Contract | Status |
|---|---|---|
| `config::watch` | Resolves the path with `std::path::absolute`, watches the parent directory, returns `(ConfigWatcher, UnboundedReceiver<Config>)` | Integrated |
| `ConfigWatcher` | `shutdown(&mut self)` is idempotent, stops the worker, and joins it. `Resolve` and `WatcherPanicked` are additive errors | Integrated |
| `AutoHideInput` | Carries injected time plus `pointer_in_dock`, `pointer_in_trigger`, `enabled`, `force_hidden`, and `pinned_open` | Integrated |
| `WindowInfo` | Keeps `hwnd`, `title`, `exe_path`, `is_minimized`, and `is_foreground` | Integrated in `src/dock/model.rs` |
| `DockItem` | Does not carry an icon. Icons stay in the UI icon cache keyed by executable path | Integrated in `src/dock/model.rs` |

The old `src/config.rs`, `src/app_state.rs`, and `src/autohide.rs` files are
removed. This section states the ownership contract only. Read `docs/HANDOFF.md`
for current verification status and pending manual evidence.

### 4.1 Stable native lifecycle contract

The lifecycle issues from the earlier re-review are resolved in source, the
independent review is cleared, and the final full gate passed on 2026-11-10.

- `worker::next_message` performs a stop-aware wait, then consumes one posted
  message with `PeekMessageW`. A sent-only wake returns to the wait without a
  blocking `GetMessageW`. `WM_QUIT` returns `None` before dispatch.
- The wait explicitly polls the manual-reset stop event with
  `WaitForSingleObject(0)` before `MsgWaitForMultipleObjectsEx`. A busy queue
  cannot starve cancellation.
- Shutdown signals a live worker before consuming either the stop handle or the
  join handle. A signal failure returns the original error and keeps both handles
  for retry. A completed worker joins without signaling. A joined panic or error
  still closes the event without aborting.
- The runtime keeps the native worker owners available for error reporting and
  retries. The logger remains the last runtime field to drop.
- Terminal cancellation failure is drop-only. If the worker is still live, the
  failure path calls lock-free taskbar recovery, writes a synchronous stderr
  diagnostic, and aborts. Completed workers and ordinary cleanup failures only
  report.
- `TaskbarOwnership` combines the state mutex with an irreversible
  `AtomicBool` recovered latch. All normal ownership checks, hides, and restores
  run under the same mutex. Recovery sets the latch before showing windows and
  does not take the state mutex.
- Every hide checks the latch before and after each native effect. An in-flight
  hide that completes after recovery compensates by force-showing its recorded
  handles. Effective ownership is the hidden flag AND `!recovered`. A late
  `TaskbarCreated` cannot hide. A startup hide after recovery returns an error.
- `TaskbarRuntime` owns the live toggle. Enable installs the panic hook, console
  handler, and watcher before hiding. Disable restores before stopping the
  watcher. The runtime and controller share it on the GPUI main thread.
- `TaskbarGuard` records a successful explicit restore so its drop does not repeat
  the native effect. A failed restore leaves the drop retry in place.

Targeted tests use production methods with fake taskbar effects. They cover both
normal mutex interleavings, a late callback, recovery between hides, and recovery
during an in-flight effect. A message-only window test covers sent-only wake and
stop, `WM_QUIT`, and a busy queue. An injected cancellation failure at the signal
operation verifies ownership retention and retry. The full gate passed on
2026-11-11 with 89 tests, clean Clippy, and a release build.

## 5. Runtime Ownership and Lifetimes

| Owner | Owns | Released by |
|---|---|---|
| `main` | Exit code and CLI dispatch | Process exit |
| Logging guard | Non-blocking log worker. Declared last in `Runtime`, so it drops last | Runtime drop or process exit |
| `SingleInstanceGuard` | Named mutex | Drop at process exit |
| `TaskbarRuntime` | Live hide/restore lifecycle, the watcher, and the one-time recovery-hook installation | `shutdown` on runtime drop |
| `AppBarHandle` | Hidden callback window, registration, reserved rect, and the AppBar thread | `disable`/`shutdown`, view drop, or process exit |
| `TaskbarGuard` | One hidden period. Ownership also includes the irreversible recovered latch | Explicit `restore`, or drop when that restore did not succeed. Panic, console, and `WM_ENDSESSION` use independent recovery paths |
| `app::runtime` | Guards, worker handles, channel receivers, config owner, strong `AppController` reference, shared `TaskbarRuntime` | Shutdown sequence |
| `AppController` | Active config, app items, config path, event subscriptions, shared `TaskbarRuntime` handle | Entity drop after the runtime releases its strong reference |
| `ConfigWatcher` | `notify` watcher and config-watch thread | Explicit `shutdown` or drop |
| `EventWorker` | Two `SetWinEventHook` handles, its stop event, and its message loop | `shutdown`, then unhook |
| `HotkeyWorker` | Registered hotkey, its stop event, and its message loop | `shutdown`, then unregister |
| `TaskbarWatcher` | Hidden watch window, its stop event, and its message loop | `shutdown` or process exit |
| `win::worker` | Manual-reset stop event, explicit stop poll, `MsgWaitForMultipleObjectsEx` wake, `PeekMessageW` message consumption, and shutdown/abort policy | Event worker, taskbar watcher, or hotkey worker |
| `DockView` | Applied config, clock entity, item snapshot, icon cache, geometry and primary-monitor snapshot, auto-hide state, menu state | Window close |
| `ClockView` | Boundary timer task | Entity drop |

The runtime keeps a strong `AppController` reference in a slot outside the view
for the whole GPUI event loop. The view observes the controller but does not own
its lifetime. The logger is the last runtime field to drop so cleanup diagnostics
reach the log writer.

The event, taskbar, and hotkey workers share `src/win/worker.rs`. They poll an
owned manual-reset event, wait with `MsgWaitForMultipleObjectsEx`, and consume one
posted message with `PeekMessageW`. Shutdown does not depend on
`PostThreadMessageW`.

`worker::shutdown` signals a live worker before consuming the stop or join handle.
A signal failure returns the original error and retains both handles for retry. A
completed worker joins without signaling. A joined panic or error still closes the
event and does not abort.

`worker::finish_failed_shutdown` is the drop-only terminal path. If the worker is
still live after a failed shutdown, it calls lock-free taskbar recovery, writes a
synchronous stderr diagnostic, and aborts. Completed workers and ordinary cleanup
failures only report. The config watcher uses its own control channel and join
handle.

### Startup Order

The order is part of the safety contract:

1. `--restore-taskbar` is handled before the single-instance check.
2. Logging starts.
3. The single-instance mutex is acquired.
4. Config is loaded and sanitized.
5. The quit hotkey and window event hook start. A missing hotkey fails startup
   only when the native taskbar will be hidden.
6. If `hide_native_taskbar` is true, `TaskbarRuntime::enable` installs the panic
   hook, console handler, taskbar watcher, and guard in that order. Console
   handler registration is required and fails startup before the taskbar is
   hidden. The guard records and suspends native taskbar auto-hide before hiding.
   If it is false, the runtime holds an empty `TaskbarRuntime` for a later live
   enable.
7. The config watcher starts.
8. The tray icon worker starts. A failure is logged and does not block the dock.
9. The GPUI application runs and the runtime retains a strong `AppController`
   reference for the whole event loop.
10. The dock window is created hidden. GPUI draws the first frame, then the window
    is shown without activation and its visibility is read back.

`TaskbarRuntime` must stay alive through `Application::run`. It is shared by the
runtime and the controller through `Rc<RefCell<...>>` on the GPUI main thread. The
taskbar watcher must exist before the guard hides the taskbar.

The settings window is not part of startup. It starts on demand from the dock
menu, the tray, or `--settings`. It runs on its own thread and owns only its
window and controls.

The AppBar reservation is created by `DockView` after the dock window opens. It is
not a runtime startup resource. The handle removes the reservation on drop, on
disable, and on process exit. `reserve_space` is ignored unless the native
taskbar is hidden.

### Shutdown Order

1. The GPUI application stops and the runtime releases its strong
   `AppController` reference.
2. Restore the taskbar and stop the watcher through `TaskbarRuntime::shutdown`.
4. Stop the config watcher.
5. Stop the window event worker and the hotkey worker.
6. Stop the tray icon worker and remove the icon.
7. Report cleanup errors without masking the startup error.
8. Release the single-instance mutex.
9. Drop the logging guard last so cleanup diagnostics are flushed.

The runtime owns these steps and keeps the native worker owners available through
error reporting and retries. The final full gate passed, but a manual normal quit
is still pending. Do not mark shutdown behavior verified until the manual quit
check passes.

## 6. Data Flow

```text
config.toml --(notify, 200 ms debounce)--> ConfigWatcher --> Config --> AppController
AppController --> DockView
WinEvent hook --(unbounded channel)--> AppController --(120 ms debounce)--> enumerate
cursor timer --> cursor + fullscreen state --> AutoHide --> progress --> UI + click-through
monitor poll --> primary work area + DPI --> dock rect + trigger --> SetWindowPos
right click --> DockView --> deferred task --> native menu --> AppController command
hide_native_taskbar --(config reload)--> AppController --> TaskbarRuntime
reserve_space + hidden taskbar --> DockView --> AppBarHandle --> ABM_SETPOS
TaskbarCreated --> taskbar watcher --> refresh hide
taskbar watch timer (2 s) --> taskbar watcher --> refresh hide
```

1. Config is loaded and sanitized before the window opens. The watcher sends a
   full `Config` after a successful parse. A failed parse keeps the last valid
   config.
2. `AppController` owns the active config and the app list. `DockView` observes
   the controller and recomputes config-derived view state only when the config
   changes.
3. Window events are coalesced for 120 ms, then `AppController` re-enumerates all
   top-level windows. Icons are cached by executable path in the view.
4. The dock polls the cursor and fullscreen state. The pure auto-hide state
   machine returns progress. The view applies offset, opacity, and click-through.
5. The native menu runs in a deferred task. `menu_open` pins the dock while the
   modal menu loop runs. Menu actions go back to `AppController`.
6. The taskbar watcher re-hides a replacement taskbar only while Dockwind owns
   the hidden state. It also refreshes every two seconds so a missed or late
   `TaskbarCreated` broadcast cannot leave the taskbar visible.

## 7. Threading Rules

| Thread | Work | Rule |
|---|---|---|
| GPUI main thread | Window, rendering, config apply, menu actions | No blocking calls in handlers |
| `win-events` | WinEvent hooks and message loop | Callback is light. No allocation or I/O in the callback. Uses the stop-aware message reader |
| `taskbar-watch` | Hidden window and message loop | Handles `WM_ENDSESSION`, `WM_CLOSE`, `TaskbarCreated`, and a two-second `WM_TIMER` refresh. Uses the stop-aware message reader |
| `quit-hotkey` | `RegisterHotKey` and message loop | Owns the hotkey registration. Uses the stop-aware message reader |
| `dockwind-tray` | Hidden window, tray callback, menu | Owns the tray icon and menu. A two-second timer verifies the icon while the taskbar is visible. Uses the stop-aware message reader |
| `dockwind-settings` | Settings window and controls | Holds the session-wide settings mutex, edits the config file on its own thread, and does not touch GPUI state |
| `dockwind-appbar` | Hidden AppBar callback window and message loop | Owns the callback window. AppBar setpos and remove calls come from the GPUI thread |
| `config-watch` | `notify` events, debounce, parse | Sends a full config to the UI. Uses its own control channel and join handle |

The native menu is modal. It must be called from a deferred task, not directly
inside a GPUI borrow. This avoids the re-entrant borrow panic recorded in D020.

The shared worker stop path is an explicit stop poll plus
`MsgWaitForMultipleObjectsEx`, followed by one `PeekMessageW` consumption per
wake. It returns to the stop-aware wait after each message, so a busy queue cannot
starve cancellation. A sent-only wake leaves no posted message and does not block.
`WM_QUIT` returns `None` before dispatch. The path does not depend on
`PostThreadMessageW`.

## 8. Invariants

1. `--restore-taskbar` stays first in `main`.
2. `TaskbarRuntime` outlives `Application::run`.
3. The taskbar watcher and console handler exist before the guard hides the
   taskbar. Console handler registration failure aborts startup before hiding.
   A live enable follows the same order.
4. The same taskbar state mutex guards the ownership flag, the visibility records,
   restore, the `TaskbarCreated` refresh, and the timer refresh. A restore that
   wins the lock is not followed by a hide.
5. Panic and console recovery use the lock-free force-show path. They may show a
   taskbar the user hid intentionally; that is preferred to a hidden desktop.
6. The recovered latch is irreversible. Effective ownership is the hidden flag
   AND `!recovered`. Every hide checks the latch before and after each native
   effect. An in-flight hide that completes after recovery compensates by
   force-showing its recorded handles.
7. `WS_EX_TOOLWINDOW` is set on the dock, `WS_EX_APPWINDOW` is clear, and the
   window has no taskbar button or Alt+Tab entry.
8. The dock does not take keyboard focus.
9. All `unsafe` stays under `src/win/` and has a `// SAFETY:` comment.
10. Pure modules do not import GPUI or Win32 types unless the type is a plain
    value such as `Bounds`.
11. Config updates run on the GPUI thread.
12. A failed config reload never replaces the last valid config.
13. A taskbar recovery path never leaves the taskbar hidden after a handled exit.
14. The dock window is not shown before GPUI has drawn its first frame. An empty
    native surface must never be visible.
15. An AppBar reservation is enabled only when `hide_native_taskbar` and
    `reserve_space` are both true. The reserved rect is removed before the
    process exits or the taskbar is restored.
16. The AppBar reserved rect starts at the dock top (bottom edge) or ends at the
    dock bottom (top edge), so maximized windows cannot cover the dock.
17. The settings window validates before saving and updates a valid existing
    TOML document in place with `toml_edit`, then writes through a same-directory
    temporary file plus an atomic rename. Comments, ordering, and unknown keys
    survive; `pinned` entries are rewritten. A failed save leaves the previous
    file, and an invalid file is not overwritten.
18. The settings window prompts before overwriting an external edit or recreating
    a deleted file.
19. The tray icon is removed on normal shutdown and re-added after
    `TaskbarCreated`. A two-second timer re-adds it if the taskbar is visible and
    the icon is missing.
20. The dock stays non-activating. The settings window is focusable and must not
    change the dock's extended styles.
21. Native taskbar auto-hide is recorded once per hide, disabled before hiding,
    and restored on normal restore and panic/console recovery. A forced kill
    leaves `%LOCALAPPDATA%\Dockwind\taskbar-auto-hide`; `--restore-taskbar`
    consumes it and restores auto-hide.
22. The taskbar refresh compares current handles with the hidden records. It
    re-hides only a replacement taskbar or a re-enabled auto-hide, so it never
    fights an explicit restore of unchanged handles.
23. The runtime holds the strong `AppController` reference for the whole event
    loop. The logger drops last.
24. The settings window holds `Local\DockwindSettings` for its lifetime. A second
    process focuses the existing window after a short retry, or takes over an
    abandoned mutex; it never opens a duplicate window.
15. The event, taskbar, and hotkey workers use the stop-aware message reader. The
    reader polls the stop event, consumes one posted message with `PeekMessageW`,
    and returns `None` for stop or `WM_QUIT`.
16. Shutdown signals a live worker before consuming its handles. Signal failure
    retains both handles. Terminal abort is drop-only and only when a live worker
    cannot be cancelled; recovery and a synchronous stderr diagnostic run first.

## 9. Known Structural Limitations

These are current limits. Do not describe them as fixed.

| Limit | Effect |
|---|---|
| Config geometry is live | `width`, `height`, `offset`, and `orientation` update without a restart. A one-second primary-monitor poll repositions on work-area, display, and DPI changes. A real multi-DPI or resolution change is not manually exercised |
| Hiding the taskbar does not update `rcWork` | The dock stays above the old work-area edge |
| AppBar reservation requires a hidden taskbar | `reserve_space` is ignored while the native taskbar is visible. A forced kill leaves the work area reserved until `--restore-taskbar` runs |
| Native auto-hide is suspended while hidden | A forced kill can leave auto-hide disabled. The marker file lets `--restore-taskbar` restore the previous setting. The watcher and tray checks run every two seconds |
| Content size defaults to `560x92` | The app list scrolls horizontally; a large list has no visible overflow affordance |
| `ApplicationFrameHost.exe` windows are skipped | UWP app identity is not mapped |
| `clock.locale` is accepted but unused | Clock text uses the process locale |
| Mica and Acrylic are gated on build 22621 and unverified | Windows 11 visual behavior is not proven on this machine |
| Running UI was exercised at 100% DPI | 125% to 200% are unit-tested conversions only |
| Manual desktop cases are unverified | Panic, console, Windows 11, Mica, and real multi-DPI are not verified on the integrated code. Normal quit, taskbar hide and restore, Explorer restart, and the restore CLI passed |

## 10. Change Rules

- Update this document when ownership, module boundaries, startup order, or
  lifetime rules change.
- Update `docs/CONFIGURATION.md` when a config field changes.
- Update `docs/PRD.md` when scope or acceptance criteria change.
- Record a deviation in `docs/decisions.md`.
- Do not create the target files as empty placeholders.
- The final full gate passed on 2026-11-10. Do not claim manual desktop behavior
  until dated evidence exists.
