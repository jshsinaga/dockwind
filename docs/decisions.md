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

Trade-off: M1 only logs reloads. M2 will need to forward the new `Config` and
`Theme` into the GPUI view, because the current watcher thread does not update
the UI.
