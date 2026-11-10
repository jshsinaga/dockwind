# Dockwind Design

Status: active. This document defines the UI and window patterns. It does not
track milestones. Read `docs/PRD.md` for scope and `docs/HANDOFF.md` for current
state and evidence.

## Design Intent

- Dockwind is a compact dock at the bottom edge of the primary display.
- It is native-adjacent. It is not a Start menu, system tray, or notification
  center replacement.
- The default look is calm: a translucent dark surface, high-contrast text, and
  small running and active indicators.
- The dock is a tool window. It does not compete with the user's active app.

## Window Model

| Property | Value | Reason |
|---|---|---|
| GPUI window kind | `WindowKind::PopUp` | Creates a tool window without a taskbar button |
| Decoration | Borderless, transparent, no title bar | The dock draws its own surface |
| Size | Fixed `560x92` logical pixels | Content is a single row with a clock |
| Position | Bottom center of the primary work area | Keeps the dock above the taskbar until taskbar hiding is enabled |
| Bottom margin | `appearance.margin_bottom`, default 12 | Visual separation from the work-area edge |
| Extended style | `WS_EX_TOOLWINDOW`, `WS_EX_TOPMOST`, `WS_EX_NOACTIVATE` | No taskbar button, stays above normal windows, does not take focus |
| App window style | `WS_EX_APPWINDOW` cleared | No Alt+Tab entry |
| Focus | `focus: false` plus restore of the previous foreground window | Clicks reach the dock without stealing focus |
| Move, resize, minimize | Disabled | The dock is a fixed control surface |
| Hidden state | `WS_EX_TRANSPARENT` at zero progress | The invisible topmost window does not catch clicks |
| Backdrop | `none` by default | Windows 10 has no supported DWM backdrop |
| Placement | Computed once at startup | Live display and work-area changes need a restart |

Placement uses the primary monitor work area. The full display bounds are not
used while the native taskbar is visible. This is a known limitation, not a
design choice for the final state.

## Layout

```text
+--------------------------------------------------+
| [app] [app] [app] ...                    HH:MM   |
|                                          Day Date|
+--------------------------------------------------+
```

- One horizontal row.
- App items take the left and center space.
- The clock is right-aligned in two lines: time above, date below.
- Content padding comes from `appearance.padding`, default 8.
- The dock does not scroll. A large app list overflows the fixed content width.

## Visual Tokens

| Token | Default | Rule |
|---|---|---|
| Background | `#1E1E2ECC` | RGBA hex, 6 or 8 digits |
| Text | White or black | Chosen by relative luminance and contrast ratio against the background |
| Running indicator | Text color at 45% alpha | Small `18x3` rounded bar under the icon |
| Active indicator | Text color at full alpha | Same bar, stronger than the running state |
| Corner radius | 16 | Capped at half of `icon_size + 2 * padding` |
| Padding | 8 | Applied inside the dock surface |
| Icon size | 40 | Allowed range 16 to 128 |
| Bottom margin | 12 | Used at window creation |
| Border | 1 px white at 20% alpha | Separates the dock from a light background |
| Hover | White at 8% alpha | Applies to an app item |

The text color is derived, not configured. A dark background gets white text and
a light background gets black text. The choice uses the WCAG relative luminance
formula and compares the two contrast ratios.

Do not add gradients, heavy shadows, decorative animation, or a second accent
color. The dock has one surface and one hierarchy.

## App Item

- The icon is loaded from the live window, then the window class, then the
  executable path.
- If no icon is available, the item shows the first letter of the app name on a
  rounded placeholder.
- The running or active indicator is always present as a 3 px-high bar. It is
  transparent when the app is pinned but not running.
- Left click activates or restores a running app, or minimizes it when it is
  already foreground. A pinned app that is not running is launched.
- Hover shows a subtle background. Tooltips, middle-click launch, per-window
  selection, and scrolling are not implemented.
- Apps are grouped by executable path. The group uses the most recent
  non-minimized window as its target.

## Clock

- Two right-aligned lines.
- `clock.time_format` controls the time. The default is `%H:%M`.
- `clock.date_format` controls the date. The default is `%a, %d %b %Y`.
- The timer aligns to the next minute, or the next second when the time format
  contains `%S`, `%T`, or `%r`.
- `clock.locale` is accepted but unused. The clock uses the process locale.

## Auto-Hide

- The trigger is a 2 px strip at the bottom of the monitor work area.
- Default show delay is 80 ms. Default hide delay is 400 ms. Default animation is
  180 ms.
- Progress from 0 to 1 drives vertical offset and opacity.
- At progress 0, the window is click-through.
- An open context menu pins the dock visible.
- A fullscreen app forces the dock hidden when `hide_in_fullscreen` is true.
- The dock polls the cursor at 100 ms while hidden and 16 ms while visible or
  animating. It does not install a low-level mouse hook.

## Backdrop and Corners

| Mode | Behavior |
|---|---|
| `none` | Per-pixel alpha and custom-drawn radius. This is the required Windows 10 mode |
| `mica` | DWM system backdrop on Windows 11 build 22621 or newer |
| `acrylic` | DWM transient backdrop on Windows 11 build 22621 or newer |

On an older build, Dockwind logs that the backdrop is unavailable and keeps the
transparent surface. The Mica and Acrylic branches are not verified on this
machine. Free radius in those modes is not guaranteed because DWM controls the
window shape.

## Context Menu

The right-click menu is a native Win32 popup, not a GPUI element. It contains:

1. Reload config.
2. Enable autostart or Disable autostart, based on the current registry state.
3. Restore taskbar and Quit.

The menu call is deferred by one task so the GPUI app borrow is not active while
the modal menu loop runs. The dock sets `menu_open` and treats it as pinned while
the menu is open.

## Future Settings Window

Not implemented. The future settings window follows these patterns:

- A separate, focusable GPUI window. It is not topmost, not click-through, and
  not `WS_EX_NOACTIVATE`.
- The same color, radius, padding, and typography tokens as the dock.
- Draft state with Apply and Cancel. Apply validates and saves. Cancel discards.
- Field-level validation errors and a clear restart notice for startup-only keys.
- One shared configuration owner. The settings window does not write the file
  directly.

## Future Installer

Not implemented. The future installer follows these patterns:

- User-scope installation is the recommendation. A machine-wide install needs a
  separate decision.
- A stable install path. The exact path is not chosen yet.
- Optional autostart through the existing per-user Run key.
- Upgrade, shutdown, taskbar restore, and uninstall flows must preserve the
  taskbar and the user's configuration unless the user explicitly removes it.
- No packaging tool is selected yet. Do not guess one in the design.

## Interaction and Accessibility

- The dock is mouse-driven. It has no keyboard focus by design.
- Text contrast is automatic, but the user cannot override it.
- Hit targets are the icon plus item padding. They do not shrink below the icon
  size.
- Tooltips and keyboard navigation are future work. The settings window is the
  first focusable surface.
- Do not add hover-only actions that have no click equivalent.

## Boundaries

- Do not add a Start replacement, tray, notification center, notification
  badges, plugin UI, or themes.
- Do not assume Mica or Acrylic works on Windows 10.
- Do not exceed the radius cap or invent a second window shape.
- Do not describe an unimplemented state as a current pattern. Mark future work
  as future.
