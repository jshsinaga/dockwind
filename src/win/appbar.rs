use std::ffi::c_void;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use tracing::{debug, info, warn};
use windows::{
    Win32::{
        Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, RECT},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA,
                SHAppBarMessage,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, RegisterClassW,
                TranslateMessage, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::win::worker;

const CLASS_NAME: PCWSTR = w!("DockwindAppBar");
const CALLBACK_MESSAGE: u32 = 0x8000 + 4;
const READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppBarEdge {
    Top,
    Bottom,
}

/// Owns the hidden callback window and the AppBar registration.
pub struct AppBarHandle {
    hwnd: HWND,
    stop: Option<isize>,
    thread: Option<JoinHandle<Result<()>>>,
    active: bool,
}

impl AppBarHandle {
    pub fn spawn() -> Result<Self> {
        let stop = worker::create_stop_signal()?;
        let (ready_sender, ready_receiver) = mpsc::channel();
        let thread = match std::thread::Builder::new()
            .name("dockwind-appbar".to_string())
            .spawn(move || run(stop, ready_sender))
        {
            Ok(thread) => thread,
            Err(error) => {
                if let Err(cleanup_error) = worker::close_stop_signal(stop) {
                    warn!(%cleanup_error, "failed to close the appbar stop signal");
                }
                return Err(anyhow!("failed to start the appbar thread: {error}"));
            }
        };

        let mut handle = Self {
            hwnd: HWND::default(),
            stop: Some(stop),
            thread: Some(thread),
            active: false,
        };

        match ready_receiver.recv_timeout(READY_TIMEOUT) {
            Ok(Ok(hwnd)) => {
                handle.hwnd = HWND(hwnd as *mut c_void);
                Ok(handle)
            }
            Ok(Err(message)) => {
                let _ = handle.shutdown();
                Err(anyhow!(message))
            }
            Err(error) => {
                let _ = handle.shutdown();
                Err(anyhow!("appbar thread did not become ready: {error}"))
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn enable(&mut self, rect: RECT, edge: AppBarEdge) -> Result<()> {
        if self.active {
            return self.update(rect, edge);
        }

        let mut data = self.data(edge, rect);
        // SAFETY: `data` is initialized with the required size and the callback
        // window is live on the appbar thread.
        let registered = unsafe { SHAppBarMessage(ABM_NEW, &mut data) };
        if registered == 0 {
            bail!("SHAppBarMessage(ABM_NEW) failed");
        }

        // SAFETY: the registration above makes this handle a live appbar.
        let queried = unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
        if queried == 0 {
            self.remove();
            bail!("SHAppBarMessage(ABM_QUERYPOS) failed");
        }

        // SAFETY: the query updated the rect and the registration is live.
        let positioned = unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
        if positioned == 0 {
            self.remove();
            bail!("SHAppBarMessage(ABM_SETPOS) failed");
        }

        self.active = true;
        info!(
            edge = ?edge,
            left = data.rc.left,
            top = data.rc.top,
            right = data.rc.right,
            bottom = data.rc.bottom,
            "appbar space reserved"
        );
        Ok(())
    }

    pub fn update(&mut self, rect: RECT, edge: AppBarEdge) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let mut data = self.data(edge, rect);
        // SAFETY: the appbar is registered and the callback window is live.
        let queried = unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
        if queried == 0 {
            bail!("SHAppBarMessage(ABM_QUERYPOS) failed during update");
        }
        // SAFETY: the query updated the rect and the appbar is registered.
        let positioned = unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
        if positioned == 0 {
            bail!("SHAppBarMessage(ABM_SETPOS) failed during update");
        }
        debug!(
            edge = ?edge,
            left = data.rc.left,
            top = data.rc.top,
            right = data.rc.right,
            bottom = data.rc.bottom,
            "appbar position updated"
        );
        Ok(())
    }

    pub fn disable(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.remove();
        self.active = false;
        info!("appbar space released");
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<()> {
        let disable_error = self.disable().err();
        let worker_result = worker::shutdown(&mut self.stop, &mut self.thread, worker::signal_stop);
        match (disable_error, worker_result) {
            (Some(error), _) => Err(error),
            (None, result) => result,
        }
    }

    fn data(&self, edge: AppBarEdge, rect: RECT) -> APPBARDATA {
        APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            uCallbackMessage: CALLBACK_MESSAGE,
            uEdge: match edge {
                AppBarEdge::Top => ABE_TOP,
                AppBarEdge::Bottom => ABE_BOTTOM,
            },
            rc: rect,
            lParam: LPARAM(0),
        }
    }

    fn remove(&mut self) {
        if self.hwnd.0.is_null() {
            return;
        }
        let mut data = self.data(AppBarEdge::Bottom, RECT::default());
        // SAFETY: the callback window is live. Removal is idempotent enough for a
        // failed registration and a normal release.
        let removed = unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
        if removed == 0 {
            warn!("SHAppBarMessage(ABM_REMOVE) returned false");
        }
    }
}

impl Drop for AppBarHandle {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            warn!(%error, "failed to release the appbar");
        }
    }
}

fn run(stop: isize, ready_sender: mpsc::Sender<Result<isize, String>>) -> Result<()> {
    // SAFETY: no window is created yet, so the module handle is stable.
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let instance = windows::Win32::Foundation::HINSTANCE(module.0);

    if let Err(error) = register_class(instance) {
        let _ = ready_sender.send(Err(error.to_string()));
        return Err(error);
    }

    // SAFETY: the class is registered and the window has no parent. It stays hidden
    // and exists only as the AppBar callback window.
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS_NAME,
            &HSTRING::from("Dockwind appbar"),
            WS_POPUP,
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
            let _ = ready_sender.send(Err(format!("CreateWindowExW(appbar) failed: {error}")));
            return Err(error).context("CreateWindowExW(appbar) failed");
        }
    };

    let _ = ready_sender.send(Ok(hwnd.0 as isize));

    while let Some(message) = worker::next_message(stop)? {
        // SAFETY: next_message returns an initialized message other than WM_QUIT.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    // SAFETY: the window belongs to this thread and is destroyed once.
    unsafe {
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
    // SAFETY: `class` is fully initialized and lives for the call.
    let atom = unsafe { RegisterClassW(&class) };
    if atom == 0 {
        // SAFETY: GetLastError has no preconditions.
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            bail!("RegisterClassW(DockwindAppBar) failed: {}", error.0);
        }
    }
    Ok(())
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // The appbar callback notifications are informational for this dock. The
    // monitor poll and the explicit geometry update own placement.
    // SAFETY: default processing for every message.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
