use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use tracing::{info, warn};
use windows::{
    Win32::{
        Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, POINT, WPARAM},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
                NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CheckMenuItem, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                DestroyMenu, DestroyWindow, DispatchMessageW, FindWindowW, GetCursorPos, HICON,
                IDI_APPLICATION, IsWindowVisible, KillTimer, LoadIconW, MF_BYCOMMAND, MF_CHECKED,
                MF_SEPARATOR, MF_STRING, MF_UNCHECKED, PostMessageW, PostQuitMessage,
                RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetTimer,
                SetWindowLongPtrW, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN,
                TrackPopupMenu, TranslateMessage, WINDOW_STYLE, WM_CONTEXTMENU, WM_DESTROY,
                WM_LBUTTONDBLCLK, WM_NULL, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW,
                WS_OVERLAPPED,
            },
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::win::{autostart, worker};

const WM_TRAY: u32 = 0x8000 + 1;
const TRAY_ID: u32 = 1;
const ICON_ID: u32 = 2;
const MENU_SETTINGS: usize = 1;
const MENU_RELOAD: usize = 2;
const MENU_AUTOSTART: usize = 3;
const MENU_RESTORE: usize = 4;
const MENU_QUIT: usize = 5;
const CLASS_NAME: PCWSTR = w!("DockwindTray");
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const REFRESH_TIMER_ID: usize = 1;
const REFRESH_INTERVAL_MS: u32 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    OpenSettings,
    ReloadConfig,
    ToggleAutostart,
    RestoreTaskbar,
    Quit,
}

/// Owns the tray icon thread. Call [`TrayIcon::shutdown`] before process exit so
/// the icon is removed and the thread is joined.
pub struct TrayIcon {
    stop: Option<isize>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl TrayIcon {
    pub fn shutdown(&mut self) -> Result<()> {
        worker::shutdown(&mut self.stop, &mut self.thread, worker::signal_stop)
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        if self.thread.is_some()
            && let Err(error) = self.shutdown()
        {
            warn!(%error, "failed to shut down the tray icon");
        }
    }
}

struct TrayState {
    sender: UnboundedSender<TrayEvent>,
    taskbar_created: u32,
    hwnd: HWND,
    icon: HICON,
}

/// Starts the tray icon and returns its event receiver.
pub fn spawn() -> Result<(TrayIcon, UnboundedReceiver<TrayEvent>)> {
    let (sender, receiver) = unbounded();
    let stop = worker::create_stop_signal()?;
    let (ready_sender, ready_receiver) = mpsc::channel();

    let thread = match std::thread::Builder::new()
        .name("dockwind-tray".to_string())
        .spawn(move || run(stop, sender, ready_sender))
    {
        Ok(thread) => thread,
        Err(error) => {
            if let Err(cleanup_error) = worker::close_stop_signal(stop) {
                warn!(%cleanup_error, "failed to close the tray stop signal");
            }
            return Err(anyhow!("failed to start the tray thread: {error}"));
        }
    };

    let mut tray = TrayIcon {
        stop: Some(stop),
        thread: Some(thread),
    };

    match ready_receiver.recv_timeout(READY_TIMEOUT) {
        Ok(Ok(())) => Ok((tray, receiver)),
        Ok(Err(message)) => {
            if let Err(error) = tray.shutdown() {
                warn!(%error, "failed to clean up the tray thread");
            }
            Err(anyhow!(message))
        }
        Err(error) => {
            if let Err(cleanup_error) = tray.shutdown() {
                warn!(%cleanup_error, "failed to clean up the tray thread");
            }
            Err(anyhow!("tray thread did not become ready: {error}"))
        }
    }
}

fn run(
    stop: isize,
    sender: UnboundedSender<TrayEvent>,
    ready_sender: mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    // SAFETY: no window is created yet, so the module handle is stable.
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let instance = windows::Win32::Foundation::HINSTANCE(module.0);

    if let Err(error) = register_class(instance) {
        let _ = ready_sender.send(Err(error.to_string()));
        return Err(error);
    }

    // SAFETY: the class is registered and the window has no parent. It stays hidden
    // and is used only as the tray callback and menu owner.
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS_NAME,
            &HSTRING::from("Dockwind tray"),
            WINDOW_STYLE(WS_OVERLAPPED.0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            let _ = ready_sender.send(Err(format!("CreateWindowExW(tray) failed: {error}")));
            return Err(error).context("CreateWindowExW(tray) failed");
        }
    };

    // SAFETY: LoadIconW reads the embedded icon resource. A missing resource falls
    // back to the stock application icon.
    let icon = unsafe { LoadIconW(Some(instance), PCWSTR(ICON_ID as *const u16)) }
        .or_else(|_| unsafe { LoadIconW(None, IDI_APPLICATION) })
        .context("failed to load the tray icon")?;

    // SAFETY: RegisterWindowMessageW has no preconditions. Zero means failure.
    let taskbar_created = unsafe { RegisterWindowMessageW(&HSTRING::from("TaskbarCreated")) };
    if taskbar_created == 0 {
        let _ = ready_sender.send(Err(
            "RegisterWindowMessageW(TaskbarCreated) failed".to_string()
        ));
        bail!("RegisterWindowMessageW(TaskbarCreated) failed");
    }

    let state = Box::new(TrayState {
        sender,
        taskbar_created,
        hwnd,
        icon,
    });
    let state = Box::into_raw(state);
    // SAFETY: the window is live and the pointer is owned until WM_DESTROY.
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            state as isize,
        )
    };

    // SAFETY: the window is live and the icon handle is valid.
    if let Err(error) = unsafe { add_icon(&*state) } {
        let _ = ready_sender.send(Err(error.to_string()));
        // SAFETY: the window was created by this thread and is destroyed once.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }

    // SAFETY: the window is live and owned by this thread.
    if unsafe { SetTimer(Some(hwnd), REFRESH_TIMER_ID, REFRESH_INTERVAL_MS, None) } == 0 {
        warn!("failed to install the tray icon refresh timer");
    }

    let _ = ready_sender.send(Ok(()));
    info!("tray icon added");

    while let Some(message) = worker::next_message(stop)? {
        // SAFETY: next_message returns an initialized message other than WM_QUIT.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    // SAFETY: the window is still live on this thread and the state pointer is
    // owned until destroy.
    unsafe {
        let _ = KillTimer(Some(hwnd), REFRESH_TIMER_ID);
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd, icon));
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}

fn register_class(instance: windows::Win32::Foundation::HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hbrBackground: HBRUSH(std::ptr::null_mut()),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialized and lives for the duration of the call.
    let atom = unsafe { RegisterClassW(&class) };
    if atom == 0 {
        // SAFETY: GetLastError has no preconditions.
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            bail!("RegisterClassW(DockwindTray) failed: {}", error.0);
        }
    }
    Ok(())
}

fn icon_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    let tip = "Dockwind";
    for (index, unit) in tip.encode_utf16().take(data.szTip.len() - 1).enumerate() {
        data.szTip[index] = unit;
    }
    data
}

unsafe fn add_icon(state: &TrayState) -> Result<()> {
    // SAFETY: the window and icon handles are live, and `data` lives for the call.
    let added = unsafe { Shell_NotifyIconW(NIM_ADD, &icon_data(state.hwnd, state.icon)) };
    if !added.as_bool() {
        bail!("Shell_NotifyIconW(NIM_ADD) failed");
    }
    Ok(())
}

unsafe fn restore_icon(state: &TrayState) -> Result<()> {
    // A failed or repeated TaskbarCreated can leave a stale icon registered. Remove
    // it before adding the replacement; a missing icon makes NIM_DELETE fail
    // harmlessly.
    // SAFETY: the window and icon handles are live, and the data lives for the call.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(state.hwnd, state.icon));
        add_icon(state)
    }
}

unsafe fn ensure_icon(state: &TrayState) -> Result<()> {
    // The shell cannot resolve an icon rectangle while the taskbar is hidden, and a
    // hidden taskbar does not need a visible icon. Verify only while it is shown.
    // SAFETY: plain lookup of the shell taskbar window.
    let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }) else {
        return Ok(());
    };
    // SAFETY: `taskbar` is a live top-level window returned by the shell.
    if !unsafe { IsWindowVisible(taskbar) }.as_bool() {
        return Ok(());
    }

    let identifier = NOTIFYICONIDENTIFIER {
        cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: state.hwnd,
        uID: TRAY_ID,
        guidItem: Default::default(),
    };
    // SAFETY: `identifier` describes this tray window and the query only reads the
    // shell's registration.
    if unsafe { Shell_NotifyIconGetRect(&identifier) }.is_ok() {
        return Ok(());
    }
    // SAFETY: the window and icon handles are live.
    unsafe { restore_icon(state) }
}

fn show_menu(state: &TrayState) -> Option<TrayEvent> {
    let autostart_enabled = autostart::is_enabled().unwrap_or_else(|error| {
        warn!(%error, "failed to query autostart state");
        false
    });

    // SAFETY: CreatePopupMenu has no preconditions; null means failure.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    let result = (|| -> Option<TrayEvent> {
        // SAFETY: `menu` is live and every HSTRING lives until the call copies it.
        unsafe {
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_SETTINGS,
                &HSTRING::from("Settings..."),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_RELOAD,
                &HSTRING::from("Reload config"),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_AUTOSTART,
                &HSTRING::from("Start with Windows"),
            )
            .ok()?;
            let _ = CheckMenuItem(
                menu,
                MENU_AUTOSTART as u32,
                MF_BYCOMMAND.0
                    | if autostart_enabled {
                        MF_CHECKED.0
                    } else {
                        MF_UNCHECKED.0
                    },
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                MENU_RESTORE,
                &HSTRING::from("Restore taskbar"),
            )
            .ok()?;
            AppendMenuW(menu, MF_STRING, MENU_QUIT, &HSTRING::from("Quit")).ok()?;
        }

        let mut cursor = POINT::default();
        // SAFETY: `cursor` is writable.
        unsafe { GetCursorPos(&mut cursor) }.ok()?;
        // SAFETY: setting the owner foreground lets the popup dismiss on an outside
        // click. Failure is tolerated; the menu still runs.
        unsafe {
            let _ = SetForegroundWindow(state.hwnd);
        };

        // SAFETY: the menu is live, the owner is the tray window, and the coordinates
        // came from GetCursorPos.
        let command = unsafe {
            TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
                cursor.x,
                cursor.y,
                None,
                state.hwnd,
                None,
            )
        };
        // SAFETY: the tray window is live. WM_NULL is the documented way to make the
        // popup dismiss cleanly after a return-command menu.
        unsafe {
            let _ = PostMessageW(Some(state.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        };

        Some(match command.0 as usize {
            MENU_SETTINGS => TrayEvent::OpenSettings,
            MENU_RELOAD => TrayEvent::ReloadConfig,
            MENU_AUTOSTART => TrayEvent::ToggleAutostart,
            MENU_RESTORE => TrayEvent::RestoreTaskbar,
            MENU_QUIT => TrayEvent::Quit,
            _ => return None,
        })
    })();

    // SAFETY: `menu` was created above and is destroyed once.
    if let Err(error) = unsafe { DestroyMenu(menu) } {
        warn!(%error, "failed to destroy the tray menu");
    }
    result
}

fn send_event(state: &TrayState, event: TrayEvent) {
    if state.sender.unbounded_send(event).is_err() {
        warn!("tray event receiver is closed");
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: WM_CREATE is not used to install the pointer; `run` installs it right
    // after CreateWindowExW returns. Other messages can arrive before that, so a
    // null pointer is handled by falling through to DefWindowProcW.
    let state_ptr = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        )
    } as *mut TrayState;
    let state = if state_ptr.is_null() {
        None
    } else {
        // SAFETY: the pointer is owned by this window until WM_DESTROY.
        Some(unsafe { &*state_ptr })
    };

    if message == WM_TRAY {
        if let Some(state) = state {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_LBUTTONDBLCLK {
                send_event(state, TrayEvent::OpenSettings);
            } else if matches!(event, WM_RBUTTONUP | WM_CONTEXTMENU)
                && let Some(action) = show_menu(state)
            {
                send_event(state, action);
            }
        }
        return windows::Win32::Foundation::LRESULT(0);
    }

    if message == WM_TIMER {
        if let Some(state) = state {
            // SAFETY: the tray window and icon handles are live for this window's lifetime.
            let result = unsafe { ensure_icon(state) };
            if let Err(error) = result {
                warn!(%error, "failed to refresh the tray icon");
            }
        }
        return windows::Win32::Foundation::LRESULT(0);
    }

    if let Some(state) = state
        && message == state.taskbar_created
    {
        info!("TaskbarCreated received; restoring the tray icon");
        // SAFETY: the window and icon handles are live. Re-adding the icon after an
        // Explorer restart is the documented tray recovery.
        if let Err(error) = unsafe { restore_icon(state) } {
            warn!(%error, "failed to restore the tray icon after TaskbarCreated");
        }
        return windows::Win32::Foundation::LRESULT(0);
    }

    match message {
        WM_DESTROY => {
            // SAFETY: the pointer was installed by `run` and is owned until now.
            unsafe {
                let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                ) as *mut TrayState;
                if !ptr.is_null() {
                    SetWindowLongPtrW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                        0,
                    );
                    drop(Box::from_raw(ptr));
                }
            }
            // SAFETY: PostQuitMessage ends this thread's loop.
            unsafe { PostQuitMessage(0) };
            windows::Win32::Foundation::LRESULT(0)
        }
        _ => {
            // SAFETY: default processing for every other message.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}
