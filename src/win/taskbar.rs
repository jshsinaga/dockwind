use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use tracing::{info, warn};
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        System::{Console::SetConsoleCtrlHandler, LibraryLoader::GetModuleHandleW},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
            GetClassNameW, GetMessageW, IsWindowVisible, MSG, RegisterClassW,
            RegisterWindowMessageW, SW_HIDE, SW_SHOW, ShowWindow, TranslateMessage, WM_CLOSE,
            WM_ENDSESSION, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::{HSTRING, PCWSTR},
};

use windows::core::BOOL;

static HIDDEN_BY_DOCKWIND: AtomicBool = AtomicBool::new(false);
static RECORDS: Mutex<Vec<TaskbarRecord>> = Mutex::new(Vec::new());
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct TaskbarRecord {
    hwnd: isize,
    was_visible: bool,
}

pub struct TaskbarGuard;

impl TaskbarGuard {
    pub fn new() -> Result<Self> {
        hide_native()?;
        Ok(Self)
    }
}

impl Drop for TaskbarGuard {
    fn drop(&mut self) {
        if let Err(error) = restore_native(false) {
            warn!(%error, "failed to restore the native taskbar on drop");
        }
    }
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_native(false);
        previous(info);
    }));
}

pub fn install_console_handler() -> Result<()> {
    unsafe { SetConsoleCtrlHandler(Some(console_handler), true) }
        .context("SetConsoleCtrlHandler failed")
}

pub fn install_watch() -> Result<()> {
    let (ready_sender, ready_receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("taskbar-watch".to_string())
        .spawn(move || unsafe {
            let module = match GetModuleHandleW(None) {
                Ok(module) => module,
                Err(error) => {
                    let _ =
                        ready_sender.send(Err(anyhow::anyhow!("GetModuleHandleW failed: {error}")));
                    return;
                }
            };
            let instance = windows::Win32::Foundation::HINSTANCE(module.0);
            let class_name = HSTRING::from("DockwindTaskbarWatch");
            let window_name = HSTRING::new();
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(taskbar_watch_proc),
                hInstance: instance,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            if RegisterClassW(&window_class) == 0 {
                let _ = ready_sender.send(Err(anyhow::anyhow!(
                    "RegisterClassW failed: {}",
                    GetLastError().0
                )));
                return;
            }

            let _window = match CreateWindowExW(
                WS_EX_TOOLWINDOW,
                &class_name,
                &window_name,
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            ) {
                Ok(window) => window,
                Err(error) => {
                    let _ =
                        ready_sender.send(Err(anyhow::anyhow!("CreateWindowExW failed: {error}")));
                    return;
                }
            };

            let taskbar_created = RegisterWindowMessageW(&HSTRING::from("TaskbarCreated"));
            if taskbar_created == 0 {
                let _ = ready_sender.send(Err(anyhow::anyhow!(
                    "RegisterWindowMessageW(TaskbarCreated) failed"
                )));
                return;
            }
            let _ = TASKBAR_CREATED.set(taskbar_created);
            let _ = ready_sender.send(Ok(()));

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        })
        .map_err(|error| anyhow::anyhow!("failed to start taskbar watcher: {error}"))?;

    match ready_receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error),
        Err(error) => bail!("taskbar watcher did not start: {error}"),
    }
}

pub fn hide_native() -> Result<()> {
    let handles = taskbar_handles();
    if handles.is_empty() {
        bail!("no native taskbar windows were found");
    }

    let mut records = RECORDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    records.clear();

    for hwnd in handles {
        let was_visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        records.push(TaskbarRecord {
            hwnd: hwnd.0 as isize,
            was_visible,
        });
    }

    HIDDEN_BY_DOCKWIND.store(true, Ordering::SeqCst);
    info!(count = records.len(), "native taskbar hidden");
    Ok(())
}

pub fn restore_native(force: bool) -> Result<()> {
    let was_hidden = HIDDEN_BY_DOCKWIND.swap(false, Ordering::SeqCst);
    if !was_hidden && !force {
        return Ok(());
    }

    let handles = taskbar_handles();
    let mut records = RECORDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    for hwnd in handles {
        let was_visible = records
            .iter()
            .find(|record| record.hwnd == hwnd.0 as isize)
            .map(|record| record.was_visible)
            .unwrap_or(true);
        if force || was_visible {
            let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
        }
    }

    records.clear();
    info!("native taskbar restored");
    Ok(())
}

fn taskbar_handles() -> Vec<HWND> {
    let mut handles = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_taskbar_proc),
            LPARAM(&mut handles as *mut Vec<HWND> as isize),
        );
    }
    handles
}

unsafe extern "system" fn enum_taskbar_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    let mut class_name = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut class_name) };
    if length > 0 {
        let class = String::from_utf16_lossy(&class_name[..length as usize]);
        if class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd" {
            handles.push(hwnd);
        }
    }
    BOOL(1)
}

unsafe extern "system" fn taskbar_watch_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_ENDSESSION && wparam.0 != 0 {
        let _ = restore_native(false);
    } else if message == WM_CLOSE {
        let _ = restore_native(false);
        let _ = unsafe { DestroyWindow(hwnd) };
        return LRESULT(0);
    } else if TASKBAR_CREATED
        .get()
        .is_some_and(|created| message == *created && HIDDEN_BY_DOCKWIND.load(Ordering::SeqCst))
        && let Err(error) = hide_native()
    {
        warn!(%error, "failed to hide the restarted taskbar");
    }

    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn console_handler(_control_type: u32) -> BOOL {
    let _ = restore_native(false);
    BOOL(0)
}
