# Changelog

All notable changes to Dockwind are recorded here. Versions follow
`MAJOR.MINOR.PATCH` from `Cargo.toml`.

## 0.3.0 - 2026-11-11

### Added

- Redesigned and modular GPUI settings window (`src/ui/settings/`) with Appearance, Clock & Date, Behavior, and Pinned Applications tabs.
- Installed application picker modal with real-time text search, A to Z alphabet pill filtering, and application icon display.
- Windows shell shortcut (`.lnk`) resolution to target native `.exe` executables through `IShellLinkW` COM interface.
- Built-in theme preset library: Catppuccin Mocha, Frosted Glass, Nordic Frost, OLED Pure Dark, and Midnight Azure.
- Rich appearance controls: manual vs auto width mode, dock width and height steppers, 4-way container padding, corner radius, border width and color, drop shadow toggle, and opacity adjustment.
- Advanced clock customization: system font picker with typography previews, font styling (bold, italic, underline), custom strftime pattern with live cheat sheet, font color picker, and top, center, bottom vertical alignments.
- Behavior controls: auto-hide toggle, animation curve selector (Ease Out, Ease In, Ease In-Out, Linear, Ease, Bounce), trigger area height, launch on startup toggle, and AppBar space reservation toggle.
- Confirmation dialogs: warning modal before enabling native taskbar hiding with recovery instructions, and confirmation modal before removing a pinned application.
- Drag-and-drop handles and up/down reorder buttons for pinned applications.
- VSync synchronized slide animations for auto-hide transitions.
- UWP application window support mapping `ApplicationFrameHost.exe` to hosted applications.

### Fixed

- Eliminated COM apartment reference leaks in installed application scanning and shortcut resolution with RAII `ComApartmentGuard`.
- Removed redundant memory clones across config validation, dock model window grouping, and watcher shutdown tests.
- Replaced monolithic 4058-line settings implementation with clean modular architecture (`mod`, `appearance`, `behavior`, `clock`, `pinned`, `modals`, `controls`).
- Corrected initial dock surface placement, window chrome borders, and first-frame rendering.
- Dock anchored directly above the native taskbar when taskbar hiding is disabled.

## 0.2.0 - 2026-11-11

### Added

- Native Win32 settings window with appearance, behavior, clock, and pinned-app
  controls.
- Apply validation, Close discard, restart notices, and an external-edit prompt.
- System tray icon with settings, reload, autostart, restore-taskbar, and quit
  actions.
- Live config reload for appearance, clock, pinned apps, auto-hide, and native
  taskbar visibility.
- AppBar reservation for the dock strip when the native taskbar is hidden.
- Auto-hide marker recovery for forced process kills.
- TOML save preservation for comments, key ordering, and unknown keys.
- One settings window per user session through a named mutex.
- Inno Setup per-user installer with Start Menu and optional desktop shortcuts.
- GitHub Actions gate, installer artifact, tag release workflow, and manual
  release dry run.

### Fixed

- Native taskbar auto-hide is suspended while the taskbar is hidden and restored
  on normal, panic, console-control, and recovery exits.
- Explorer restarts are detected by a watcher refresh even when
  `TaskbarCreated` arrives late.
- Stale taskbar handles are reconciled during restore.
- UWP windows hosted by `ApplicationFrameHost.exe` map to the hosted app process
  instead of being skipped.
- Uninstall force-stops a running Dockwind before restoring the taskbar, so an
  uninstall while the taskbar is hidden removes the app and leaves the desktop
  usable.
- The tray icon is checked while the taskbar is visible and re-added if missing.
- Settings saves no longer replace a valid file with a fresh serialization.

## 0.1.0 - 2026-11-10

- Initial GPUI dock surface with transparent, rounded rendering.
- Primary-monitor placement with work-area and DPI conversion.
- Config file, appearance theming, clock, pinned apps, and running-app list.
- App icon extraction, activation, restore, minimize, and launch.
- Auto-hide, click-through while hidden, and native taskbar hide/restore.
- Global quit hotkey, dock context menu, autostart, and config watcher.
