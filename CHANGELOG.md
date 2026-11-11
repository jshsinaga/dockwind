# Changelog

All notable changes to Dockwind are recorded here. Versions follow
`MAJOR.MINOR.PATCH` from `Cargo.toml`.

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
