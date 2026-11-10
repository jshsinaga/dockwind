# M0 Spike: GPUI Window Feasibility

Date: 2026-11-10
Status: PASS for the Windows 10 required criteria; Windows 11 DWM backdrop remains unverified.

## Environment

| Item | Value |
|---|---|
| OS | Windows 10 Pro 22H2, build 19045 |
| Display | 1920x1080, 96 DPI (100%) |
| Rust | stable-x86_64-pc-windows-msvc 1.99.1 |
| Cargo | 1.99.1 |
| Visual Studio | Build Tools 2022 17.14.42 |
| MSVC | 14.44.35207 |
| Windows SDK | 10.0.26100.0 |
| GPUI | 0.2.2 from crates.io, exact pin |
| windows crate | 0.61.3 |
| raw-window-handle | 0.6.2 |

## Decision

Use the published `gpui` 0.2.2 crate with an exact `=0.2.2` pin, not a moving Git branch. Use `windows = "0.61"` to match the version GPUI already depends on and avoid duplicate Win32 types.

This decision is recorded here because `docs/decisions.md` is scheduled for M1.

## Spike Scope

The spike opens a borderless `WindowKind::PopUp` at the bottom of the primary display, draws its own rounded rectangle, applies Win32 styles through the raw HWND, and updates the UI from a plain OS thread.

Run with:

```powershell
cargo run
```

## Results

| Criterion | Result | Evidence |
|---|---|---|
| Transparent window | PASS | Desktop and taskbar remain visible through the dock. Corner samples match the pixels outside the rounded shape; the center blends the dock color with the background. |
| Custom rounded corners | PASS | 20 px radius drawn by GPUI. The corners are transparent and show the window behind. |
| Borderless / no native title bar | PASS | `titlebar.appears_transparent = true`; the client area is the custom-drawn dock. |
| No taskbar button | PASS | `WindowKind::PopUp` creates `WS_EX_TOOLWINDOW`; `WS_EX_APPWINDOW` is absent. |
| No Alt+Tab entry | PASS by window style | `WS_EX_TOOLWINDOW` is set, `WS_EX_APPWINDOW` is clear, and the window owner is `0`. |
| Topmost | PASS | `WS_EX_TOPMOST` is set and `SetWindowPos(HWND_TOPMOST)` succeeds. The dock stays above the taskbar and other windows. |
| Does not steal focus | PASS | `focus: false` alone was not enough. With `WS_EX_NOACTIVATE` and restoring the previous foreground window, the foreground stayed on Windows Terminal. |
| Mouse input while non-activating | PASS | A temporary click handler received one click (`M0 click count=1`) while the foreground window remained Windows Terminal. The diagnostic handler was removed after the test. |
| Bottom positioning | PASS | Client rect `560x92` at `(680, 976)` on a `1920x1080` screen with a 12 px bottom margin. |
| Wake UI from another OS thread | PASS | A std thread sends a counter every 500 ms through `futures::channel::mpsc`; `cx.spawn` receives it and calls `cx.notify()`. The visible counter reached 142. |
| DWM backdrop | NOT AVAILABLE on this OS | `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE = 38)` returns `0x80070057` (`E_INVALIDARG`) on Windows 10 19045. Expected; Mica/Acrylic requires Windows 11 22H2 or newer. |

## Win32 Details

Observed extended style after configuration:

```text
ex_style = 0x08200188
  0x08000000  WS_EX_NOACTIVATE
  0x00200000  WS_EX_NOREDIRECTIONBITMAP (set by GPUI for DirectComposition)
  0x00000100  WS_EX_WINDOWEDGE
  0x00000080  WS_EX_TOOLWINDOW
  0x00000008  WS_EX_TOPMOST
```

`WS_EX_APPWINDOW` (`0x00040000`) is absent. The window owner is `0`.

GPUI adds an invisible Win32 border around the client area. At 96 DPI:

| Measurement | Value |
|---|---|
| Requested GPUI bounds | `560x92` |
| Client rect | `560x92` |
| Window rect | `576x100` |
| Border offset | 8 px left/right, 4 px top/bottom |

M2 must use GPUI's logical bounds for content and account for this offset when positioning against the monitor work area.

## Transparency and Backdrop Findings

GPUI's `WindowBackgroundAppearance::Transparent` does not use the official DWM backdrop API. On Windows it calls `SetWindowCompositionAttribute` with accent state `2` (`ACCENT_ENABLE_TRANSPARENTGRADIENT`). This is the undocumented API the plan already identifies for Windows 10.

This spike proves that free rounded corners and per-pixel alpha work with GPUI in this mode. It does not prove the plan's `mica` or `acrylic` branches, because those need Windows 11 22H2 or newer and `DWMWA_SYSTEMBACKDROP_TYPE`.

The blur plus radius conflict remains open for Windows 11:

- `blur = "none"`: free radius works, as shown here.
- `blur = "mica" | "acrylic"`: still needs a Windows 11 test to determine whether DWM corner preference or `SetWindowRgn` is required.

## Notes for Later Milestones

1. M1 should move the pin decision into `docs/decisions.md` and replace `println!` diagnostics with `tracing`.
2. M2 must use the monitor work area, not only full display bounds, and must handle the GPUI border offset at non-100% DPI.
3. M2 must repeat the backdrop test on Windows 11 22H2 or newer before treating the Mica/Acrylic contract as valid.
4. `WS_EX_NOACTIVATE` means the dock has no keyboard focus. Context menus and any keyboard-driven dock action need explicit testing in M5/M6.
5. GPUI 0.2.2 is pre-1.0. Keep the exact pin and re-verify APIs when upgrading.

## Verdict

M0 passes the required Windows 10 criteria: transparent custom-drawn rounded window, no taskbar button, no Alt+Tab entry, topmost placement, non-activating behavior, and cross-thread UI updates. The Windows 11 Mica/Acrylic branch is not verified on this machine and must be tested before M2 closes.
