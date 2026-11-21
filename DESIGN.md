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
| Size | `appearance.width x height`, default `560x92` | Content is a single row with a clock |
| Position | Top or bottom center of the primary work area | `appearance.orientation` selects the edge |
| Offset | `appearance.offset`, default 12 | Visual separation from the work-area edge |
| Extended style | `WS_EX_TOOLWINDOW`, `WS_EX_TOPMOST`, `WS_EX_NOACTIVATE` | No taskbar button, stays above normal windows, does not take focus |
| App window style | `WS_EX_APPWINDOW` cleared | No Alt+Tab entry |
| Focus | `focus: false` plus restore of the previous foreground window | Clicks reach the dock without stealing focus |
| Move, resize, minimize | Disabled | The dock is a fixed control surface |
| Hidden state | `WS_EX_TRANSPARENT` at zero progress | The invisible topmost window does not catch clicks |
| Backdrop | `none` by default | Windows 10 has no supported DWM backdrop |
| Placement | Live for config changes | DPI, display, and work-area changes still need the next milestone |

Placement uses the primary monitor work area. The full display bounds are not
used while the native taskbar is visible. This is a known limitation, not a
design choice for the final state.

## Settings and Tray

The settings window is a separate, focusable native Win32 window. It uses
standard controls and the system dialog font. It does not reuse the dock surface
or the dock's non-activating styles. The dock stays a tool window; the settings
window is a normal application window.

The settings window groups controls in this order: Appearance, Behavior, Clock,
Pinned apps, then Apply and Close. Appearance starts with size, padding, radius,
offset, opacity, background, and orientation. Behavior keeps auto-hide and the
fullscreen setting above the startup-only taskbar and hotkey settings. Destructive
or startup-only fields carry a short inline note.

The tray icon uses the Dockwind icon. Its menu is a standard native popup with the
same actions as the dock menu plus a Start with Windows check item. The tray does
not draw custom chrome.

When `reserve_space` is enabled with a hidden taskbar, the dock owns an AppBar
reservation. The reserved strip includes the dock surface and the configured
offset. Maximized windows stop at the dock edge instead of covering it. The
reservation is not drawn; it only changes the work area.

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
- The app list scrolls horizontally beside the clock. The dock fits within the
  display work area in both fixed and automatic width modes. Chevron buttons
  page through an overflowing list when there is room for the controls.
- A top-oriented dock mirrors the layout vertically: the hidden state slides up
  and the trigger sits on the top work-area edge.

## Visual Tokens

| Token | Default | Rule |
|---|---|---|
| Background | `#1E1E2ECC` | RGBA hex, 6 or 8 digits. `opacity` multiplies the alpha |
| Text | White or black | Chosen by relative luminance and contrast ratio against the background |
| Running indicator | Text color at 45% alpha | Small `18x3` rounded bar under the icon |
| Active indicator | Text color at full alpha | Same bar, stronger than the running state |
| Corner radius | 16 | Capped at half of `icon_size + 2 * padding` |
| Padding | 8 | Applied inside the dock surface |
| Icon size | 40 | Allowed range 16 to 128 |
| Offset | 12 | Distance from the selected work-area edge |
| Width and height | 560 and 92 | Configurable and live |
| Opacity | 100 | Percentage multiplier on the background alpha |
| Border | 1 px white at 20% alpha | Separates the dock from a light background |
| Tile feedback | Text color at 8%, 13%, or 20% alpha | Foreground, hover, and press states keep the tile in place |

The text color is derived, not configured. A dark background gets white text and
a light background gets black text. The choice uses the WCAG relative luminance
formula and compares the two contrast ratios.

Do not add gradients, heavy shadows, decorative animation, or a second accent
color. The dock has one surface and one hierarchy.

## App Item

- The icon loads in the background from the live window, then the window class,
  then the executable path. A rounded placeholder with the app's first letter
  is visible while the icon loads or when no icon is available.
- The running or active indicator uses the configured line or dot style. Its
  space remains reserved when a pinned app is idle.
- Left click activates or restores a running app, or minimizes it when it is
  already foreground. A pinned app that is not running is launched.
- Middle-click or Shift+click opens a second instance of a pinned app.
- Apps are grouped by executable path. The group uses the most recent
  non-minimized window as its target.

## Clock

- Two right-aligned lines.
- `clock.time_format` controls the time. The default is `%H:%M`.
- `clock.date_format` controls the date. The default is `%a, %d %b %Y`.
- The timer aligns to the next minute, or the next second when either line
  displays seconds. Hiding the clock stops the timer.
- `clock.locale` is accepted but unused. The clock uses the process locale.

## Auto-Hide

- The trigger is a 2 px strip at the bottom of the monitor work area.
- Default show delay is 80 ms. Default hide delay is 400 ms. Default animation is
  180 ms. Turning off animation effects in Windows skips the dock transition
  while retaining its show and hide delays.
- Progress from 0 to 1 drives vertical offset and opacity.
- If the pointer returns to the dock area during a hide transition, the dock
  reverses direction. A fully hidden dock relies on the edge trigger.
- At progress 0, the window is click-through.
- An open context menu pins the dock visible.
- A fullscreen app forces the dock hidden when `hide_in_fullscreen` is true.
- The dock polls the cursor every 32 ms while idle and hidden, and every 16 ms
  while visible or animating. Cursor polling stops when auto-hide is disabled.

## Backdrop and Corners

| Mode | Behavior |
|---|---|
| `none` | Per-pixel alpha and the configured corner radius |
| `mica` | DWM system backdrop on Windows 11 build 22621 or newer, legacy blur behind on older builds |
| `acrylic` | DWM transient backdrop on Windows 11 build 22621 or newer, legacy acrylic on older builds |

The native material sits under the translucent dock tint. The window region
follows the visible rounded bar as it slides, and Windows controls the outer
corner shading of its material. A fully hidden dock clears the material and
reapplies it on reveal. A failed material request restores the plain transparent
surface and logs the Windows error.

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
