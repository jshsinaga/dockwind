# Dockwind Handoff

Last updated: 2026-11-10
Stage: final full gate passed. Manual desktop cases remain unverified.
Historical baseline: `482bd7a`

This is the starting point for a fresh agent. It records current state, pending
work, and dated evidence. It does not repeat the architecture. Read
`docs/ARCHITECTURE.md` for module ownership and lifetimes.

## Current State

| Item | Value |
|---|---|
| Product | Floating Windows 10/11 dock bar in Rust |
| UI | GPUI `=0.2.2`, native Windows target |
| Branch | `main` |
| Source layout | Target layout integrated |
| Independent review | Cleared on 2026-11-10. No new blockers |
| Tests | 66 tests passed in the final full gate on 2026-11-10 |
| Clippy | `cargo clippy --all-targets --locked -- -D warnings` passed in the final full gate |
| Intermediate gate | Historical: 59 tests passed before the lifecycle fixes. Superseded by the final gate |
| Release binary | `target\release\dockwind.exe` |
| Lints | `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks` are `deny`. Production-only `unwrap_used` and `expect_used` are denied outside tests |
| Final status | Full gate passed on 2026-11-10. Manual desktop cases unverified |

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

- Transparent, borderless, rounded dock drawn by GPUI.
- Primary work-area placement with DPI conversion.
- Live config reload for appearance, clock, pinned apps, and auto-hide.
- App list grouped by executable, with live icons.
- Left-click activate, restore, minimize, or launch.
- Clock and date aligned to the minute or second.
- Auto-hide with a 2 px trigger, animation, and click-through while hidden.
- Native taskbar hide and restore with a guard, panic hook, console handler,
  session-end handling, and `TaskbarCreated` re-hide.
- Standalone `--restore-taskbar` recovery.
- Global quit hotkey, default `Ctrl+Alt+Shift+Q`.
- Single-instance mutex.
- Native right-click menu.
- Autostart through the menu and CLI.

These are current code capabilities. Manual desktop verification for this
review-cleared code is still pending.

## Taskbar Safety

Read this before changing taskbar or worker code.

- `hide_native_taskbar` defaults to false. Enable it only for a controlled test.
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
| 2026-11-10 | Build warning | Baseline `proc-macro-error2` v2.0.1 warning remains |
| 2026-11-10 | Earlier debug startup smoke | 4 seconds on Windows 10 build 19045, 1920x1080, 100% DPI, `hide_native_taskbar = false`. Logs showed config load, hotkey registration, placement, transparent styles. Process was force-killed |

Not verified on the review-cleared integrated code:

- Manual normal quit.
- Manual taskbar hide and restore.
- Real Explorer restart.
- Panic restore.
- Console-control restore.
- `--restore-taskbar` recovery CLI.
- GUI smoke after the lifecycle fixes.
- Windows 11 behavior.
- Mica or Acrylic.
- Real multi-DPI desktop.

The final full gate passed on 2026-11-10. The evidence above applies to the
current review-cleared source.

## Pending Work

Order is a recommendation, not a commitment.

1. **Manual desktop checks.** Normal quit, taskbar hide and restore, real Explorer
   restart, panic, console control, and `--restore-taskbar`. Confirm the taskbar is
   visible afterward. See PRD U06.
2. **AppBar `reserve_space`.** Implement `SHAppBarMessage` registration,
   `ABN_POSCHANGED`, `ABN_FULLSCREENAPP`, and `ABM_REMOVE` on shutdown. See PRD
   U01.
3. **Live geometry.** Recompute placement on DPI change, display change, and
   taskbar hide or restore. See PRD U02.
4. **App-list polish.** Middle-click launch, tooltips, scrolling, and overflow.
   See PRD U03.
5. **UWP identity.** Map `ApplicationFrameHost.exe` windows to the real app, or
   document a supported alternative. See PRD U04.
6. **Windows 11 blur.** Exercise Mica and Acrylic on Windows 11 22H2 or newer.
   See PRD U05.
7. **Multi-DPI UI.** Exercise the running UI at 125%, 150%, and 200%. See PRD U07.
8. **Shutdown latency.** Profile teardown if the several-second exit remains.
   See PRD U08.
9. **Settings UI.** Future work. See PRD P01 to P07.
10. **Installer.** Future work. See PRD P08 to P13.
11. **Hosted CI.** Deferred until a host is selected. See PRD P14.

## Known Limitations

The detailed list is in `docs/PRD.md` section 5 and `docs/ARCHITECTURE.md`
section 9. The short version:

- Placement is computed once at startup.
- `reserve_space` does nothing.
- The app list does not scroll.
- UWP windows are skipped.
- `clock.locale` is unused.
- Mica and Acrylic are unverified on Windows 11.
- The running UI was exercised at 100% DPI only.
- Manual desktop recovery cases are unverified.
- Settings and installer do not exist.

## Documentation Map

| File | Role |
|---|---|
| [AGENTS.md](../AGENTS.md) | Authoritative working rules and spec pointers |
| [docs/PRD.md](PRD.md) | Scope, requirements, status, acceptance criteria |
| [docs/ARCHITECTURE.md](ARCHITECTURE.md) | Target modules, ownership, lifetimes, invariants |
| [DESIGN.md](../DESIGN.md) | UI and window patterns |
| [docs/CONFIGURATION.md](CONFIGURATION.md) | Config schema, validation, reload, save contract |
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
   2026-11-10 final gate applies to that source. Later runs are gate evidence for
   your own changes, not that source.
4. Launch with `hide_native_taskbar = false` and `auto_hide = false` for a
   predictable UI. Enable auto-hide only when testing it.
5. Pick one pending item. Write a pure test where possible. Verify manually.
6. Update the canonical doc and this handoff with dated evidence.
7. Never leave the taskbar hidden. Use the restore command and a watchdog during
   any taskbar-hiding test.
