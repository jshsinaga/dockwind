# Changelog

All notable changes to Dockwind are recorded here. Versions follow
`MAJOR.MINOR.PATCH` from `Cargo.toml`.

## 0.5.0 - 2026-11-21

### Added

- Enable icon zoom and hover background independently under Settings >
  Appearance. Zoom stays within the app tile and keeps the dock height and
  icon spacing fixed.
- Click an app with multiple windows to choose a window by title.
- Open the dock's **Tray** menu to see network, sound, battery, and input
  language status, adjust output volume, mute sound or the default microphone,
  and open Windows settings. When Dockwind hides the taskbar, this menu can
  reveal the native notification area and its hidden icons temporarily.

### Fixed

- Keep eligible windows in the dock when Windows withholds their executable
  paths. Use Windows app identities to keep distinct apps separate when they
  share an executable. Follow focus through owned dialogs and refresh when
  Windows cloaks or reveals a window.
- Clear icon hover effects when the pointer leaves the dock and restore them
  when it returns.
- Start Dockwind from Explorer, Start, or autostart without opening a terminal.
- Align Mica and Acrylic with the dock frame. Switch blur modes live and clear
  the native material by selecting **None**. Older Windows builds use their
  legacy blur and acrylic effects.

## 0.4.0 - 2026-11-21

### Added

- Save, rename, duplicate, and load named sets of pinned apps. Switch from the
  dock's right-click menu or choose **Next layout** to cycle through distinct
  sets in name order.
- Edit pinned app names and add, edit, reorder, or remove launch arguments in
  Settings. Shift+click or middle-click starts another instance with that
  pin's arguments.
- Page through overflowing app lists with chevrons that dim at the ends.
- Use Ctrl+A, Ctrl+C, Ctrl+X, Ctrl+V, and Ctrl+S in Settings. Press Enter in the
  installed-app picker to pin the sole matching result.

### Changed

- Auto-hide slides and fades together, reverses when the pointer returns
  mid-hide, and respects the Windows animation effects setting. App tiles stay
  in place on hover and show distinct hover, press, and foreground states.
- The dock stays within the display work area and reserves space for the clock
  beside the scrolling app strip. Switching layouts resets the strip to its
  first app.
- Window discovery and icon loading run in the background. The installed-app
  picker scans in the background, draws only visible rows, and searches app
  names and executable paths.
- The clock updates every second when either line shows seconds, pauses while
  hidden, and redraws only when its text changes.

### Fixed

- Settings offers to save or discard unsaved edits before closing. External
  config changes can be reloaded from Settings, and a second external edit
  requires a new overwrite confirmation.
- Config saves use separate temporary files. Autostart-only changes leave the
  config file untouched.
- Invalid clock formats show an inline preview error and can be corrected before
  saving. Focusing the background hex field preserves its alpha value.
- Duplicate pins keep independent tile state, and clicks remain tied to the
  rendered app when the running list changes.
- Native pickers report errors in Settings. File browsing uses the modern
  Windows dialog to preserve the working directory, and installed-app discovery
  skips linked directories.

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
