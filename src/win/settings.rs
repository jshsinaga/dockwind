use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use tracing::{error, info, warn};
use windows::{
    Win32::{
        Foundation::{
            COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, WPARAM,
        },
        Graphics::Gdi::{COLOR_WINDOW, DEFAULT_GUI_FONT, GetStockObject, HBRUSH},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::Dialogs::{
                CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW, CommDlgExtendedError,
                GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
            },
            WindowsAndMessaging::{
                BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_GROUPBOX, BS_PUSHBUTTON,
                CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CBS_DROPDOWNLIST, CREATESTRUCTW,
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
                FindWindowW, GWLP_USERDATA, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
                GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IDYES, LB_ADDSTRING,
                LB_GETCURSEL, LB_RESETCONTENT, LB_SETCURSEL, LBS_NOTIFY, LoadCursorW, MB_ICONERROR,
                MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_YESNO, MSG, MessageBoxW,
                PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_RESTORE, SW_SHOW,
                SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
                TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
                WM_DESTROY, WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
                WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
                WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use windows::Win32::UI::Controls::BST_CHECKED;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;

use crate::config::{Config, DockEdge, parse_rgba_hex};

const CLASS_NAME: PCWSTR = w!("DockwindSettings");
const WINDOW_WIDTH: i32 = 640;
const WINDOW_HEIGHT: i32 = 880;

const ID_WIDTH: i32 = 1001;
const ID_HEIGHT: i32 = 1002;
const ID_ICON_SIZE: i32 = 1003;
const ID_PADDING: i32 = 1004;
const ID_RADIUS: i32 = 1005;
const ID_OFFSET: i32 = 1006;
const ID_OPACITY: i32 = 1007;
const ID_BACKGROUND: i32 = 1008;
const ID_ORIENTATION: i32 = 1009;
const ID_PICK_COLOR: i32 = 1010;

const ID_AUTO_HIDE: i32 = 1101;
const ID_HIDE_FULLSCREEN: i32 = 1102;
const ID_SHOW_RUNNING: i32 = 1103;
const ID_HIDE_TASKBAR: i32 = 1104;
const ID_RESERVE_SPACE: i32 = 1105;
const ID_AUTOSTART: i32 = 1106;
const ID_QUIT_HOTKEY: i32 = 1107;

const ID_TIME_FORMAT: i32 = 1201;
const ID_DATE_FORMAT: i32 = 1202;

const ID_PINNED: i32 = 1301;
const ID_ADD_APP: i32 = 1302;
const ID_REMOVE_APP: i32 = 1303;
const ID_MOVE_UP: i32 = 1304;
const ID_MOVE_DOWN: i32 = 1305;

const ID_APPLY: i32 = 1401;
const ID_CLOSE: i32 = 1402;

static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct Controls {
    width: HWND,
    height: HWND,
    icon_size: HWND,
    padding: HWND,
    radius: HWND,
    offset: HWND,
    opacity: HWND,
    background: HWND,
    orientation: HWND,
    auto_hide: HWND,
    hide_fullscreen: HWND,
    show_running: HWND,
    hide_taskbar: HWND,
    reserve_space: HWND,
    autostart: HWND,
    quit_hotkey: HWND,
    time_format: HWND,
    date_format: HWND,
    pinned: HWND,
}

struct SettingsState {
    path: PathBuf,
    config: Config,
    original: Config,
    original_contents: String,
    pinned: Vec<crate::config::PinnedApp>,
    controls: Controls,
}

/// Opens the settings window on a dedicated thread. A second request focuses the
/// existing window instead of starting another one.
pub fn open(config_path: PathBuf) -> Result<()> {
    if SETTINGS_OPEN.swap(true, Ordering::SeqCst) {
        focus_existing();
        return Ok(());
    }

    match std::thread::Builder::new()
        .name("dockwind-settings".to_string())
        .spawn(move || {
            if let Err(error) = run(config_path) {
                error!(%error, "settings window failed");
                show_message(
                    HWND::default(),
                    &format!("{error:#}"),
                    "Dockwind settings",
                    MB_OK | MB_ICONERROR,
                );
            }
            SETTINGS_OPEN.store(false, Ordering::SeqCst);
        }) {
        Ok(_) => Ok(()),
        Err(error) => {
            SETTINGS_OPEN.store(false, Ordering::SeqCst);
            Err(anyhow!("failed to start the settings thread: {error}"))
        }
    }
}

/// Runs the settings window on the calling thread. Used by the `--settings` CLI
/// entry point, which must keep the process alive until the window closes.
pub fn open_blocking(config_path: PathBuf) -> Result<()> {
    if SETTINGS_OPEN.swap(true, Ordering::SeqCst) {
        focus_existing();
        return Ok(());
    }
    let result = run(config_path);
    SETTINGS_OPEN.store(false, Ordering::SeqCst);
    result
}

fn focus_existing() {
    // SAFETY: FindWindowW is a read-only lookup by class name. A null result means
    // no window exists.
    let Ok(hwnd) = (unsafe { FindWindowW(CLASS_NAME, PCWSTR::null()) }) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    // SAFETY: `hwnd` is a live window owned by the settings thread.
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn run(config_path: PathBuf) -> Result<()> {
    let loaded = Config::load_or_create(&config_path)?;
    for warning in &loaded.warnings {
        warn!("{warning}");
    }

    // SAFETY: no window is created yet, so the current module handle is stable.
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let instance = HINSTANCE(module.0);
    register_class(instance)?;

    let original_contents = std::fs::read_to_string(&config_path)
        .context("failed to read the config for the settings baseline")?;
    let state = SettingsState {
        path: config_path,
        config: loaded.config.clone(),
        original: loaded.config.clone(),
        original_contents,
        pinned: loaded.config.pinned.clone(),
        controls: Controls::default(),
    };
    let state_ptr = Box::into_raw(Box::new(state));

    let title = HSTRING::from(format!("Dockwind Settings {}", env!("CARGO_PKG_VERSION")));
    // SAFETY: the class is registered, the title is a live HSTRING, the parent is
    // null because this is a top-level window, and `state_ptr` stays valid until
    // WM_CREATE takes ownership.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            CLASS_NAME,
            &title,
            WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0),
            (GetSystemMetrics(SM_CXSCREEN) - WINDOW_WIDTH) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - WINDOW_HEIGHT) / 2,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            None,
            None,
            Some(instance),
            Some(state_ptr as *const c_void),
        )
    }
    .context("CreateWindowExW(settings) failed")?;

    // SAFETY: the window is live and owned by this thread.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }

    let mut message = MSG::default();
    // SAFETY: `message` is writable, and the loop dispatches only messages for this
    // thread. GetMessageW returns false for WM_QUIT and for an error.
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        // SAFETY: the message was returned by GetMessageW and not filtered out.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        // SAFETY: LoadCursorW is a read-only stock-resource lookup.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        // The system color brush is a predefined HBRUSH value, not an owned handle.
        hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut c_void),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };

    // SAFETY: `class` is fully initialized and lives until RegisterClassW copies it.
    let atom = unsafe { RegisterClassW(&class) };
    if atom == 0 {
        // SAFETY: GetLastError has no preconditions.
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            bail!("RegisterClassW(DockwindSettings) failed: {}", error.0);
        }
    }
    Ok(())
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    match message {
        WM_CREATE => {
            // SAFETY: WM_CREATE carries a CREATESTRUCTW whose lpCreateParams is the
            // Box pointer passed to CreateWindowExW.
            let state = unsafe {
                let create = &*(lparam.0 as *const CREATESTRUCTW);
                Box::from_raw(create.lpCreateParams as *mut SettingsState)
            };
            match create_controls(hwnd, &state.config) {
                Ok(controls) => {
                    let mut state = state;
                    state.controls = controls;
                    let state = Box::into_raw(state);
                    // SAFETY: the window is live and the pointer is owned until
                    // WM_DESTROY.
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                    windows::Win32::Foundation::LRESULT(0)
                }
                Err(error) => {
                    error!(%error, "failed to create the settings controls");
                    windows::Win32::Foundation::LRESULT(-1)
                }
            }
        }
        WM_COMMAND => {
            // SAFETY: the pointer is installed by WM_CREATE and cleared by WM_DESTROY.
            if let Some(state) = unsafe { state_mut(hwnd) } {
                let id = (wparam.0 & 0xFFFF) as i32;
                handle_command(hwnd, state, id);
            }
            windows::Win32::Foundation::LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: the window handle is live.
            unsafe {
                DestroyWindow(hwnd).ok();
            };
            windows::Win32::Foundation::LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: WM_CREATE installed this pointer and this is its only cleanup.
            unsafe {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SettingsState;
                if !ptr.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Box::from_raw(ptr));
                }
            }
            // SAFETY: PostQuitMessage ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            windows::Win32::Foundation::LRESULT(0)
        }
        _ => {
            // SAFETY: default processing for every other message.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

/// # Safety
/// The caller must ensure the window user data still points to a live
/// `SettingsState`. The window procedure installs it in WM_CREATE and clears it in
/// WM_DESTROY.
unsafe fn state_mut(hwnd: HWND) -> Option<&'static mut SettingsState> {
    // SAFETY: documented by the caller.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsState;
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the pointer is live for the lifetime of the window.
        Some(unsafe { &mut *ptr })
    }
}

fn handle_command(hwnd: HWND, state: &mut SettingsState, id: i32) {
    let result = match id {
        ID_APPLY => apply(hwnd, state),
        ID_CLOSE => {
            // SAFETY: the window handle is live.
            unsafe {
                DestroyWindow(hwnd).ok();
            };
            Ok(())
        }
        ID_PICK_COLOR => pick_color(hwnd, state),
        ID_ADD_APP => add_pinned(hwnd, state),
        ID_REMOVE_APP => remove_pinned(state),
        ID_MOVE_UP => move_pinned(state, -1),
        ID_MOVE_DOWN => move_pinned(state, 1),
        _ => return,
    };

    if let Err(error) = result {
        show_message(
            hwnd,
            &format!("{error:#}"),
            "Dockwind settings",
            MB_OK | MB_ICONERROR,
        );
    }
}

fn create_controls(parent: HWND, config: &Config) -> Result<Controls> {
    group(parent, "Appearance", 12, 10, 616, 268)?;
    label(parent, "Width", 24, 38)?;
    let width = edit(parent, ID_WIDTH, 250, 36, 90)?;
    label(parent, "Height", 24, 64)?;
    let height = edit(parent, ID_HEIGHT, 250, 62, 90)?;
    label(parent, "Icon size", 24, 90)?;
    let icon_size = edit(parent, ID_ICON_SIZE, 250, 88, 90)?;
    label(parent, "Padding", 24, 116)?;
    let padding = edit(parent, ID_PADDING, 250, 114, 90)?;
    label(parent, "Corner radius", 24, 142)?;
    let radius = edit(parent, ID_RADIUS, 250, 140, 90)?;
    label(parent, "Offset", 24, 168)?;
    let offset = edit(parent, ID_OFFSET, 250, 166, 90)?;
    label(parent, "Opacity %", 24, 194)?;
    let opacity = edit(parent, ID_OPACITY, 250, 192, 90)?;
    label(parent, "Background", 24, 220)?;
    let background = edit(parent, ID_BACKGROUND, 250, 218, 250)?;
    button(parent, ID_PICK_COLOR, "Pick color", 510, 217, 90)?;
    label(parent, "Orientation", 24, 246)?;
    let orientation = combo(parent, ID_ORIENTATION, 250, 244, 120)?;
    add_combo_item(orientation, "Bottom")?;
    add_combo_item(orientation, "Top")?;

    group(parent, "Behavior", 12, 286, 616, 240)?;
    let auto_hide = checkbox(
        parent,
        ID_AUTO_HIDE,
        "Auto-hide when not used",
        24,
        314,
        260,
    )?;
    let hide_fullscreen = checkbox(
        parent,
        ID_HIDE_FULLSCREEN,
        "Hide during fullscreen apps",
        24,
        340,
        260,
    )?;
    let show_running = checkbox(parent, ID_SHOW_RUNNING, "Show running apps", 24, 366, 260)?;
    let hide_taskbar = checkbox(parent, ID_HIDE_TASKBAR, "Hide native taskbar", 24, 392, 360)?;
    let reserve_space = checkbox(
        parent,
        ID_RESERVE_SPACE,
        "Reserve screen space (not implemented)",
        24,
        418,
        360,
    )?;
    let autostart = checkbox(parent, ID_AUTOSTART, "Start with Windows", 24, 444, 260)?;
    label(parent, "Quit hotkey (restart required)", 24, 470)?;
    let quit_hotkey = edit(parent, ID_QUIT_HOTKEY, 250, 468, 200)?;

    group(parent, "Clock", 12, 536, 616, 90)?;
    label(parent, "Time format", 24, 564)?;
    let time_format = edit(parent, ID_TIME_FORMAT, 250, 562, 200)?;
    label(parent, "Date format", 24, 590)?;
    let date_format = edit(parent, ID_DATE_FORMAT, 250, 588, 200)?;

    group(parent, "Pinned apps", 12, 636, 616, 150)?;
    let pinned = control(
        parent,
        ControlSpec {
            class: w!("LISTBOX"),
            text: "",
            style: WINDOW_STYLE(
                WS_CHILD.0
                    | WS_VISIBLE.0
                    | WS_BORDER.0
                    | WS_VSCROLL.0
                    | WS_TABSTOP.0
                    | LBS_NOTIFY as u32,
            ),
            ex_style: WS_EX_CLIENTEDGE,
            x: 24,
            y: 664,
            width: 430,
            height: 105,
            id: ID_PINNED,
        },
    )?;
    button(parent, ID_ADD_APP, "Add app...", 470, 664, 140)?;
    button(parent, ID_REMOVE_APP, "Remove", 470, 696, 140)?;
    button(parent, ID_MOVE_UP, "Up", 470, 728, 66)?;
    button(parent, ID_MOVE_DOWN, "Down", 544, 728, 66)?;

    button(parent, ID_APPLY, "Apply", 400, 800, 100)?;
    button(parent, ID_CLOSE, "Close", 510, 800, 100)?;

    let controls = Controls {
        width,
        height,
        icon_size,
        padding,
        radius,
        offset,
        opacity,
        background,
        orientation,
        auto_hide,
        hide_fullscreen,
        show_running,
        hide_taskbar,
        reserve_space,
        autostart,
        quit_hotkey,
        time_format,
        date_format,
        pinned,
    };

    // SAFETY: every control is owned by the parent window and live. The return
    // value is the previous enabled state, not success.
    unsafe {
        let _ = EnableWindow(reserve_space, false);
    }
    populate_controls(&controls, config);
    Ok(controls)
}

fn populate_controls(controls: &Controls, config: &Config) {
    set_text(controls.width, &config.appearance.width.to_string());
    set_text(controls.height, &config.appearance.height.to_string());
    set_text(controls.icon_size, &config.appearance.icon_size.to_string());
    set_text(controls.padding, &config.appearance.padding.to_string());
    set_text(
        controls.radius,
        &config.appearance.corner_radius.to_string(),
    );
    set_text(controls.offset, &config.appearance.offset.to_string());
    set_text(controls.opacity, &config.appearance.opacity.to_string());
    set_text(controls.background, &config.appearance.background);
    set_combo(
        controls.orientation,
        match config.appearance.orientation {
            DockEdge::Bottom => 0,
            DockEdge::Top => 1,
        },
    );
    set_check(controls.auto_hide, config.behavior.auto_hide);
    set_check(controls.hide_fullscreen, config.behavior.hide_in_fullscreen);
    set_check(controls.show_running, config.behavior.show_running_apps);
    set_check(controls.hide_taskbar, config.behavior.hide_native_taskbar);
    set_check(controls.reserve_space, config.behavior.reserve_space);
    set_text(controls.quit_hotkey, &config.behavior.quit_hotkey);
    set_text(controls.time_format, &config.clock.time_format);
    set_text(controls.date_format, &config.clock.date_format);

    let autostart = match crate::win::autostart::is_enabled() {
        Ok(enabled) => enabled,
        Err(error) => {
            warn!(%error, "failed to query autostart state");
            false
        }
    };
    set_check(controls.autostart, autostart);
    refresh_pinned(controls, &config.pinned, None);
}

fn refresh_pinned(
    controls: &Controls,
    pinned: &[crate::config::PinnedApp],
    selected: Option<usize>,
) {
    send_message(controls.pinned, LB_RESETCONTENT, 0, 0);
    for app in pinned {
        let text = HSTRING::from(&app.name);
        send_message(controls.pinned, LB_ADDSTRING, 0, text.as_ptr() as isize);
    }
    if let Some(index) = selected {
        send_message(controls.pinned, LB_SETCURSEL, index, 0);
    }
}

fn apply(hwnd: HWND, state: &mut SettingsState) -> Result<()> {
    let config = read_config(state)?;
    let startup_only_changed = config.behavior.quit_hotkey != state.original.behavior.quit_hotkey;

    match std::fs::read_to_string(&state.path) {
        Ok(contents) if contents != state.original_contents => {
            if !confirm(
                hwnd,
                "The config file changed on disk. Overwrite it?",
                "Dockwind settings",
            ) {
                return Ok(());
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !confirm(
                hwnd,
                "The config file is missing. Recreate it?",
                "Dockwind settings",
            ) {
                return Ok(());
            }
        }
        Err(error) => return Err(error).context("failed to read the config before saving"),
    }

    let loaded = config
        .save_atomic(&state.path)
        .context("failed to save the configuration; the existing file is unchanged")?;

    state.config = loaded.config.clone();
    state.pinned = loaded.config.pinned.clone();
    if let Ok(contents) = std::fs::read_to_string(&state.path) {
        state.original_contents = contents;
    } else {
        warn!("failed to refresh the settings baseline after saving");
    }
    populate_controls(&state.controls, &loaded.config);

    let autostart_desired = get_check(state.controls.autostart);
    let autostart_current = crate::win::autostart::is_enabled().unwrap_or(false);
    if autostart_desired != autostart_current {
        let result = if autostart_desired {
            crate::win::autostart::enable()
        } else {
            crate::win::autostart::disable()
        };
        if let Err(error) = result {
            show_message(
                hwnd,
                &format!("Settings were saved, but autostart could not be updated: {error:#}"),
                "Dockwind settings",
                MB_OK | MB_ICONERROR,
            );
        }
    }

    let mut notes = Vec::new();
    if startup_only_changed {
        notes.push("Restart Dockwind to apply the new quit hotkey.".to_string());
    }
    if !loaded.warnings.is_empty() {
        notes.push(format!(
            "Some values were corrected:\n{}",
            loaded.warnings.join("\n")
        ));
    }
    if !notes.is_empty() {
        show_message(
            hwnd,
            &notes.join("\n\n"),
            "Dockwind settings",
            MB_OK | MB_ICONINFORMATION,
        );
    }

    info!(path = %state.path.display(), "settings saved");
    Ok(())
}

fn read_config(state: &SettingsState) -> Result<Config> {
    let controls = &state.controls;
    let mut config = state.config.clone();

    config.appearance.width = parse_u32(controls.width, "width")?;
    config.appearance.height = parse_u32(controls.height, "height")?;
    config.appearance.icon_size = parse_u32(controls.icon_size, "icon size")?;
    config.appearance.padding = parse_u32(controls.padding, "padding")?;
    config.appearance.corner_radius = parse_u32(controls.radius, "corner radius")?;
    config.appearance.offset = parse_u32(controls.offset, "offset")?;
    let opacity = parse_u32(controls.opacity, "opacity")?;
    config.appearance.opacity = u8::try_from(opacity).context("opacity must be 0..255")?;
    config.appearance.background = get_text(controls.background);
    if parse_rgba_hex(&config.appearance.background).is_none() {
        bail!("background must be #RRGGBB or #RRGGBBAA");
    }
    config.appearance.orientation = match get_combo(controls.orientation) {
        1 => DockEdge::Top,
        _ => DockEdge::Bottom,
    };

    config.behavior.auto_hide = get_check(controls.auto_hide);
    config.behavior.hide_in_fullscreen = get_check(controls.hide_fullscreen);
    config.behavior.show_running_apps = get_check(controls.show_running);
    config.behavior.hide_native_taskbar = get_check(controls.hide_taskbar);
    config.behavior.reserve_space = get_check(controls.reserve_space);
    config.behavior.quit_hotkey = get_text(controls.quit_hotkey);
    crate::win::hotkey::validate(&config.behavior.quit_hotkey)
        .map_err(|error| anyhow!("invalid quit hotkey: {error}"))?;

    config.clock.time_format = get_text(controls.time_format);
    config.clock.date_format = get_text(controls.date_format);
    config.pinned = state.pinned.clone();
    Ok(config)
}

fn add_pinned(hwnd: HWND, state: &mut SettingsState) -> Result<()> {
    let Some(path) = pick_app(hwnd)? else {
        return Ok(());
    };
    let path_text = path.to_string_lossy().to_string();
    if state
        .pinned
        .iter()
        .any(|app| app.path.eq_ignore_ascii_case(&path_text))
    {
        bail!("that app is already pinned");
    }

    let name = path
        .file_stem()
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path_text.clone());
    state.pinned.push(crate::config::PinnedApp {
        name,
        path: path_text,
        args: Vec::new(),
    });
    refresh_pinned(&state.controls, &state.pinned, Some(state.pinned.len() - 1));
    Ok(())
}

fn remove_pinned(state: &mut SettingsState) -> Result<()> {
    let Some(index) = selected_pinned(state) else {
        return Ok(());
    };
    state.pinned.remove(index);
    let selected = if state.pinned.is_empty() {
        None
    } else {
        Some(index.min(state.pinned.len() - 1))
    };
    refresh_pinned(&state.controls, &state.pinned, selected);
    Ok(())
}

fn move_pinned(state: &mut SettingsState, delta: isize) -> Result<()> {
    let Some(index) = selected_pinned(state) else {
        return Ok(());
    };
    let target = index as isize + delta;
    if target < 0 || target as usize >= state.pinned.len() {
        return Ok(());
    }
    state.pinned.swap(index, target as usize);
    refresh_pinned(&state.controls, &state.pinned, Some(target as usize));
    Ok(())
}

fn selected_pinned(state: &SettingsState) -> Option<usize> {
    let index = send_message(state.controls.pinned, LB_GETCURSEL, 0, 0);
    if index.0 < 0 {
        None
    } else {
        Some(index.0 as usize)
    }
}

fn pick_app(parent: HWND) -> Result<Option<PathBuf>> {
    let mut buffer = [0u16; 2048];
    let filter: Vec<u16> = "Applications\0*.exe;*.lnk;*.bat;*.cmd\0All files\0*.*\0\0"
        .encode_utf16()
        .collect();
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: parent,
        lpstrFilter: windows::core::PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };

    // SAFETY: every pointer in `dialog` refers to a live buffer for the duration of
    // the call. The dialog blocks until the user closes it.
    let accepted = unsafe { GetOpenFileNameW(&mut dialog) }.as_bool();
    if !accepted {
        // SAFETY: CommDlgExtendedError has no preconditions. Zero means the user
        // cancelled rather than an error.
        let error = unsafe { CommDlgExtendedError() };
        if error.0 != 0 {
            bail!("file picker failed with code {}", error.0);
        }
        return Ok(None);
    }

    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..length],
    ))))
}

fn pick_color(parent: HWND, state: &mut SettingsState) -> Result<()> {
    let current = get_text(state.controls.background);
    let [red, green, blue, _] = parse_rgba_hex(&current)
        .ok_or_else(|| anyhow!("background must be #RRGGBB or #RRGGBBAA"))?;
    let mut custom = [COLORREF(0xFFFFFF); 16];
    let mut dialog = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: parent,
        rgbResult: COLORREF((red as u32) | ((green as u32) << 8) | ((blue as u32) << 16)),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..Default::default()
    };

    // SAFETY: every pointer in `dialog` refers to a live buffer for the duration of
    // the call. The dialog blocks until the user closes it.
    if !unsafe { ChooseColorW(&mut dialog) }.as_bool() {
        return Ok(());
    }

    let value = dialog.rgbResult.0;
    let color = format!(
        "#{:02X}{:02X}{:02X}",
        value & 0xFF,
        (value >> 8) & 0xFF,
        (value >> 16) & 0xFF
    );
    set_text(state.controls.background, &color);
    Ok(())
}

fn parse_u32(hwnd: HWND, field: &str) -> Result<u32> {
    let text = get_text(hwnd);
    text.trim()
        .parse::<u32>()
        .with_context(|| format!("{field} must be a whole number"))
}

struct ControlSpec<'a> {
    class: PCWSTR,
    text: &'a str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: i32,
}

fn control(parent: HWND, spec: ControlSpec<'_>) -> Result<HWND> {
    let ControlSpec {
        class,
        text,
        style,
        ex_style,
        x,
        y,
        width,
        height,
        id,
    } = spec;

    // SAFETY: the current module handle is stable for the process lifetime.
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let text = HSTRING::from(text);
    // SAFETY: the parent is live, the class is a predefined system class, the text
    // is a live HSTRING, and the control id is passed as the menu handle.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class,
            &text,
            style,
            x,
            y,
            width,
            height,
            Some(parent),
            Some(HMENU(id as *mut c_void)),
            Some(HINSTANCE(module.0)),
            None,
        )
    }
    .with_context(|| format!("failed to create control {id}"))?;

    // SAFETY: the control is live. DEFAULT_GUI_FONT is a process-owned stock object.
    unsafe {
        let font = GetStockObject(DEFAULT_GUI_FONT);
        send_message(hwnd, WM_SETFONT, font.0 as usize, 1);
    }
    Ok(hwnd)
}

fn group(parent: HWND, text: &str, x: i32, y: i32, width: i32, height: i32) -> Result<HWND> {
    control(
        parent,
        ControlSpec {
            class: w!("BUTTON"),
            text,
            style: WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | BS_GROUPBOX as u32),
            ex_style: WINDOW_EX_STYLE(0),
            x,
            y,
            width,
            height,
            id: 0,
        },
    )
}

fn label(parent: HWND, text: &str, x: i32, y: i32) -> Result<HWND> {
    control(
        parent,
        ControlSpec {
            class: w!("STATIC"),
            text,
            style: WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
            ex_style: WINDOW_EX_STYLE(0),
            x,
            y,
            width: 210,
            height: 20,
            id: 0,
        },
    )
}

fn edit(parent: HWND, id: i32, x: i32, y: i32, width: i32) -> Result<HWND> {
    control(
        parent,
        ControlSpec {
            class: w!("EDIT"),
            text: "",
            style: WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32),
            ex_style: WS_EX_CLIENTEDGE,
            x,
            y,
            width,
            height: 22,
            id,
        },
    )
}

fn combo(parent: HWND, id: i32, x: i32, y: i32, width: i32) -> Result<HWND> {
    control(
        parent,
        ControlSpec {
            class: w!("COMBOBOX"),
            text: "",
            style: WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | CBS_DROPDOWNLIST as u32),
            ex_style: WINDOW_EX_STYLE(0),
            x,
            y,
            width,
            height: 120,
            id,
        },
    )
}

fn checkbox(parent: HWND, id: i32, text: &str, x: i32, y: i32, width: i32) -> Result<HWND> {
    control(
        parent,
        ControlSpec {
            class: w!("BUTTON"),
            text,
            style: WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32),
            ex_style: WINDOW_EX_STYLE(0),
            x,
            y,
            width,
            height: 22,
            id,
        },
    )
}

fn button(parent: HWND, id: i32, text: &str, x: i32, y: i32, width: i32) -> Result<HWND> {
    control(
        parent,
        ControlSpec {
            class: w!("BUTTON"),
            text,
            style: WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_PUSHBUTTON as u32),
            ex_style: WINDOW_EX_STYLE(0),
            x,
            y,
            width,
            height: 28,
            id,
        },
    )
}
fn send_message(
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: the caller passes a live control owned by this window.
    unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
}

fn set_text(hwnd: HWND, text: &str) {
    let text = HSTRING::from(text);
    // SAFETY: `hwnd` is an edit control owned by the settings window.
    if let Err(error) = unsafe { SetWindowTextW(hwnd, &text) } {
        warn!(%error, "failed to set a settings control value");
    }
}

fn get_text(hwnd: HWND) -> String {
    // SAFETY: `hwnd` is a live settings control.
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    // SAFETY: the buffer has `length + 1` units, as required by GetWindowTextW.
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..written.max(0) as usize])
}

fn set_check(hwnd: HWND, checked: bool) {
    let value = if checked { BST_CHECKED.0 as usize } else { 0 };
    send_message(hwnd, BM_SETCHECK, value, 0);
}

fn get_check(hwnd: HWND) -> bool {
    let state = send_message(hwnd, BM_GETCHECK, 0, 0);
    state.0 == BST_CHECKED.0 as isize
}

fn set_combo(hwnd: HWND, index: usize) {
    send_message(hwnd, CB_SETCURSEL, index, 0);
}

fn get_combo(hwnd: HWND) -> usize {
    let index = send_message(hwnd, CB_GETCURSEL, 0, 0);
    index.0.max(0) as usize
}

fn add_combo_item(hwnd: HWND, text: &str) -> Result<()> {
    let text = HSTRING::from(text);
    let index = send_message(hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
    if index.0 < 0 {
        bail!("failed to add a combo box item");
    }
    Ok(())
}

fn confirm(owner: HWND, text: &str, title: &str) -> bool {
    let text = HSTRING::from(text);
    let title = HSTRING::from(title);
    // SAFETY: both strings are live for the call and the owner may be null.
    let result = unsafe { MessageBoxW(Some(owner), &text, &title, MB_YESNO | MB_ICONWARNING) };
    result.0 == IDYES.0
}

fn show_message(
    owner: HWND,
    text: &str,
    title: &str,
    style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) {
    let text = HSTRING::from(text);
    let title = HSTRING::from(title);
    // SAFETY: both strings are live for the call and the owner may be null.
    unsafe {
        MessageBoxW(Some(owner), &text, &title, style);
    }
}
