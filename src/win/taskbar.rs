use std::ffi::c_void;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use tracing::{info, warn};
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        System::{Console::SetConsoleCtrlHandler, LibraryLoader::GetModuleHandleW},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
            GetClassNameW, IsWindowVisible, RegisterClassW, RegisterWindowMessageW, SW_HIDE,
            SW_SHOW, ShowWindow, TranslateMessage, UnregisterClassW, WM_ENDSESSION, WNDCLASSW,
            WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::{BOOL, HSTRING, PCWSTR},
};

use crate::win::worker;

const WATCH_CLASS_NAME: &str = "DockwindTaskbarWatch";
const READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
struct TaskbarRecord {
    hwnd: isize,
    was_visible: bool,
}

/// Taskbar ownership and the visibility recorded when Dockwind hid the taskbar.
/// Every normal check and effect holds the same mutex so a restore and a
/// `TaskbarCreated` re-hide cannot interleave.
#[derive(Debug)]
struct TaskbarState {
    hidden_by_dockwind: bool,
    records: Vec<TaskbarRecord>,
}

struct TaskbarOwnership {
    state: Mutex<TaskbarState>,
    recovered: AtomicBool,
}

static TASKBAR_OWNERSHIP: TaskbarOwnership = TaskbarOwnership::new();

impl TaskbarOwnership {
    const fn new() -> Self {
        Self {
            state: Mutex::new(TaskbarState {
                hidden_by_dockwind: false,
                records: Vec::new(),
            }),
            recovered: AtomicBool::new(false),
        }
    }

    fn hide(&self, windows: &dyn TaskbarWindows, only_if_owned: bool) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Ownership includes the permanent recovery latch. A callback arriving after
        // recovery must never re-hide, even if the mutex-protected flag is still set.
        if self.recovered.load(Ordering::SeqCst) {
            if only_if_owned {
                return Ok(());
            }
            bail!("taskbar hiding is disabled after emergency recovery");
        }
        if only_if_owned && !state.hidden_by_dockwind {
            return Ok(());
        }
        hide_locked(&mut state, windows, &self.recovered)
    }

    fn restore(&self, windows: &dyn TaskbarWindows, force: bool) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        restore_locked(
            &mut state,
            windows,
            force || self.recovered.load(Ordering::SeqCst),
        )
    }

    fn recover(&self, windows: &dyn TaskbarWindows) -> Result<()> {
        // Never acquire state here: a panic can interrupt its owner. The latch is
        // never reset. Every native hide checks it AFTER the effect as well: a hide
        // completing after this show must repair its own effect before returning.
        self.recovered.store(true, Ordering::SeqCst);
        let handles = windows.handles()?;
        if handles.is_empty() {
            bail!("no native taskbar windows were found during forced recovery");
        }
        let mut failures = 0;
        for hwnd in handles {
            windows.set_visible(hwnd, true);
            if !windows.is_visible(hwnd) {
                failures += 1;
            }
        }
        if failures != 0 {
            bail!("failed to show {failures} native taskbar window(s) during recovery");
        }
        Ok(())
    }
}

static TASKBAR_CREATED_MESSAGE: Mutex<Option<u32>> = Mutex::new(None);

/// Narrow access to the native taskbar windows, so the ownership transitions can be
/// tested without hiding the real taskbar.
trait TaskbarWindows {
    fn handles(&self) -> Result<Vec<isize>>;
    fn is_visible(&self, hwnd: isize) -> bool;
    fn set_visible(&self, hwnd: isize, visible: bool);
}

struct NativeTaskbar;

impl TaskbarWindows for NativeTaskbar {
    fn handles(&self) -> Result<Vec<isize>> {
        Ok(taskbar_handles()?
            .into_iter()
            .map(|hwnd| hwnd.0 as isize)
            .collect())
    }

    fn is_visible(&self, hwnd: isize) -> bool {
        let hwnd = HWND(hwnd as *mut c_void);
        // SAFETY: `hwnd` is a taskbar window handle from `taskbar_handles`; the query
        // is read-only.
        unsafe { IsWindowVisible(hwnd) }.as_bool()
    }

    fn set_visible(&self, hwnd: isize, visible: bool) {
        let hwnd = HWND(hwnd as *mut c_void);
        // SAFETY: ShowWindow accepts any window handle. Its return value reports the
        // previous visibility, not success, so callers verify with `is_visible`.
        unsafe {
            let _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        }
    }
}

/// Owns the hidden taskbar lifecycle. Restores on drop unless an explicit restore
/// already succeeded.
pub struct TaskbarGuard {
    restored: bool,
}

impl TaskbarGuard {
    /// Hides the native taskbar after recording which windows were visible.
    pub fn new() -> Result<Self> {
        hide_native()?;
        Ok(Self { restored: false })
    }

    /// Restores the taskbar and reports failures. Dropping the guard retries and logs
    /// only when this explicit restore did not succeed.
    pub fn restore(&mut self) -> Result<()> {
        restore_native(false)?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for TaskbarGuard {
    fn drop(&mut self) {
        if self.restored {
            return;
        }
        if let Err(error) = restore_native(false) {
            warn!(%error, "failed to restore the native taskbar on drop");
        }
    }
}

/// Owns the taskbar watcher thread that reacts to `TaskbarCreated` and session end.
pub struct TaskbarWatcher {
    stop: Option<isize>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl TaskbarWatcher {
    /// Signals the stop event and joins the watcher. Safe to call more than once.
    pub fn shutdown(&mut self) -> Result<()> {
        worker::shutdown(&mut self.stop, &mut self.thread, worker::signal_stop)
    }
}

impl Drop for TaskbarWatcher {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            worker::finish_failed_shutdown(&mut self.stop, &mut self.thread, &error);
            warn!(%error, "failed to stop the taskbar watcher on drop");
        }
    }
}

/// Owns the live taskbar hide/watch lifecycle. The runtime and the controller share
/// this on the GPUI main thread. Enabling installs the recovery hooks and the
/// watcher before hiding; disabling restores before stopping the watcher.
#[derive(Default)]
pub struct TaskbarRuntime {
    watcher: Option<TaskbarWatcher>,
    guard: Option<TaskbarGuard>,
    hooks_installed: bool,
}

impl TaskbarRuntime {
    pub fn enable(&mut self) -> Result<()> {
        if self.guard.is_some() {
            return Ok(());
        }

        if !self.hooks_installed {
            install_panic_hook();
            install_console_handler()?;
            self.hooks_installed = true;
        }

        // A failed disable can leave the watcher alive. Stop it before replacing it.
        if let Some(mut watcher) = self.watcher.take() {
            watcher.shutdown()?;
        }

        let watcher = install_watch()?;
        match TaskbarGuard::new() {
            Ok(guard) => {
                self.watcher = Some(watcher);
                self.guard = Some(guard);
                info!("native taskbar hidden");
                Ok(())
            }
            Err(error) => {
                let mut watcher = watcher;
                if let Err(cleanup_error) = watcher.shutdown() {
                    warn!(%cleanup_error, "failed to stop the watcher after a hide failure");
                }
                Err(error)
            }
        }
    }

    pub fn disable(&mut self) -> Result<()> {
        if let Some(mut guard) = self.guard.take() {
            guard.restore()?;
        }
        if let Some(mut watcher) = self.watcher.take() {
            watcher.shutdown()?;
        }
        info!("native taskbar restored");
        Ok(())
    }

    pub fn shutdown(&mut self) -> Vec<anyhow::Error> {
        let mut errors = Vec::new();
        if let Some(mut guard) = self.guard.take()
            && let Err(error) = guard.restore()
        {
            errors.push(error);
        }
        if let Some(mut watcher) = self.watcher.take()
            && let Err(error) = watcher.shutdown()
        {
            errors.push(error.context("taskbar watcher shutdown failed"));
        }
        errors
    }
}

impl Drop for TaskbarRuntime {
    fn drop(&mut self) {
        // Explicit shutdown wins. This fallback keeps restore-before-watcher-stop
        // order even when the runtime is dropped without calling it.
        if let Some(mut guard) = self.guard.take()
            && let Err(error) = guard.restore()
        {
            warn!(%error, "failed to restore the native taskbar on runtime drop");
        }
        if let Some(mut watcher) = self.watcher.take()
            && let Err(error) = watcher.shutdown()
        {
            warn!(%error, "failed to stop the taskbar watcher on runtime drop");
        }
    }
}

/// Restores the taskbar before a panic continues, then runs the previous hook. This
/// path does not take the state lock, so a panic while the lock is held cannot
/// deadlock recovery.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Err(error) = force_show_taskbars() {
            warn!(%error, "failed to restore the native taskbar from the panic hook");
        }
        previous(info);
    }));
}

/// Restores the taskbar when the console receives Ctrl+C, close, logoff, or shutdown.
/// The handler runs on a system thread and uses the lock-free force path.
pub fn install_console_handler() -> Result<()> {
    // SAFETY: the handler is a plain `extern "system"` function that does not unwind.
    // Windows stores the pointer for the process lifetime.
    unsafe { SetConsoleCtrlHandler(Some(console_handler), true) }
        .context("SetConsoleCtrlHandler failed")
}

/// Starts the taskbar watcher thread.
pub fn install_watch() -> Result<TaskbarWatcher> {
    let stop = worker::create_stop_signal()?;
    let (ready_sender, ready_receiver) = mpsc::channel();
    let thread = match std::thread::Builder::new()
        .name("taskbar-watch".to_string())
        .spawn(move || run(stop, ready_sender))
    {
        Ok(thread) => thread,
        Err(error) => {
            if let Err(cleanup_error) = worker::close_stop_signal(stop) {
                warn!(%cleanup_error, "failed to close the taskbar watcher stop signal");
            }
            return Err(anyhow!("failed to start taskbar watcher: {error}"));
        }
    };

    let mut watcher = TaskbarWatcher {
        stop: Some(stop),
        thread: Some(thread),
    };

    match ready_receiver.recv_timeout(READY_TIMEOUT) {
        Ok(Ok(())) => Ok(watcher),
        Ok(Err(message)) => {
            if let Err(cleanup_error) = watcher.shutdown() {
                warn!(%cleanup_error, "failed to clean up the taskbar watcher");
            }
            Err(anyhow!(message))
        }
        Err(error) => {
            if let Err(cleanup_error) = watcher.shutdown() {
                warn!(%cleanup_error, "failed to clean up the taskbar watcher");
            }
            Err(anyhow!("taskbar watcher did not become ready: {error}"))
        }
    }
}

fn run(stop: isize, ready_sender: mpsc::Sender<Result<(), String>>) -> Result<()> {
    // SAFETY: GetModuleHandleW(None) returns the module for the current executable.
    let module = match unsafe { GetModuleHandleW(None) } {
        Ok(module) => module,
        Err(error) => {
            let message = format!("GetModuleHandleW failed: {error}");
            let _ = ready_sender.send(Err(message.clone()));
            return Err(anyhow!(message));
        }
    };
    let instance = windows::Win32::Foundation::HINSTANCE(module.0);
    let class_name = HSTRING::from(WATCH_CLASS_NAME);
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(taskbar_watch_proc),
        hInstance: instance,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    // SAFETY: the class name and window procedure stay alive for the thread. Windows
    // copies the strings it needs. A zero atom means registration failed.
    if unsafe { RegisterClassW(&window_class) } == 0 {
        // SAFETY: GetLastError has no preconditions.
        let code = unsafe { GetLastError() };
        let message = format!("RegisterClassW failed: error {}", code.0);
        let _ = ready_sender.send(Err(message.clone()));
        return Err(anyhow!(message));
    }

    let window_name = HSTRING::new();
    // SAFETY: `instance` owns the registered class; every string outlives the call.
    let window = match unsafe {
        CreateWindowExW(
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
        )
    } {
        Ok(window) => window,
        Err(error) => {
            // SAFETY: the class was registered above on this thread and no window of
            // this class exists yet.
            if let Err(cleanup_error) = unsafe { UnregisterClassW(&class_name, Some(instance)) } {
                warn!(%cleanup_error, "failed to unregister a partially created taskbar watcher");
            }
            let message = format!("CreateWindowExW failed: {error}");
            let _ = ready_sender.send(Err(message.clone()));
            return Err(anyhow!(message));
        }
    };

    // SAFETY: plain call with a valid, null-terminated class name. Zero means failure.
    let taskbar_created = unsafe { RegisterWindowMessageW(&HSTRING::from("TaskbarCreated")) };
    if taskbar_created == 0 {
        // SAFETY: both the window and its class were created on this thread.
        if let Err(error) = unsafe { DestroyWindow(window) } {
            warn!(%error, "failed to destroy a partially created taskbar watcher window");
        }
        // SAFETY: the class was registered on this thread and its only window is gone.
        if let Err(error) = unsafe { UnregisterClassW(&class_name, Some(instance)) } {
            warn!(%error, "failed to unregister a partially created taskbar watcher class");
        }
        let message = "RegisterWindowMessageW(TaskbarCreated) failed".to_string();
        let _ = ready_sender.send(Err(message.clone()));
        return Err(anyhow!(message));
    }
    *TASKBAR_CREATED_MESSAGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(taskbar_created);

    let _ = ready_sender.send(Ok(()));

    let loop_result = message_loop(stop);

    // SAFETY: the window and class were created on this thread and are released on the
    // same thread. Failure to clean up is reported but does not replace a loop error.
    if let Err(error) = unsafe { DestroyWindow(window) } {
        warn!(%error, "failed to destroy the taskbar watch window");
    }
    // SAFETY: the class was registered on this thread and its only window is gone.
    let class_result = unsafe { UnregisterClassW(&class_name, Some(instance)) };
    if let Err(error) = &class_result {
        warn!(%error, "failed to unregister the taskbar watch window class");
    }
    *TASKBAR_CREATED_MESSAGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

    match loop_result {
        Ok(()) => class_result.map_err(|error| anyhow!("{error}")),
        Err(error) => Err(error),
    }
}

fn message_loop(stop: isize) -> Result<()> {
    while let Some(message) = worker::next_message(stop)? {
        // SAFETY: next_message returned an initialized message other than WM_QUIT.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

/// Whether a taskbar window should be shown during restore.
fn needs_show(force: bool, hidden_by_dockwind: bool, was_visible: bool) -> bool {
    force || (hidden_by_dockwind && was_visible)
}

/// Hides every native taskbar window and records its previous visibility.
pub fn hide_native() -> Result<()> {
    TASKBAR_OWNERSHIP.hide(&NativeTaskbar, false)
}

/// Shows the taskbar windows hidden by Dockwind, or every taskbar window when forced.
pub fn restore_native(force: bool) -> Result<()> {
    TASKBAR_OWNERSHIP.restore(&NativeTaskbar, force)
}

fn hide_locked(
    state: &mut TaskbarState,
    windows: &dyn TaskbarWindows,
    recovered: &AtomicBool,
) -> Result<()> {
    let handles = windows.handles()?;
    if handles.is_empty() {
        bail!("no native taskbar windows were found");
    }

    state.records.clear();

    for hwnd in handles {
        let was_visible = windows.is_visible(hwnd);
        state.records.push(TaskbarRecord { hwnd, was_visible });
        if recovered.load(Ordering::SeqCst) {
            restore_records(state, windows, true, true)?;
            state.hidden_by_dockwind = false;
            bail!("taskbar hide cancelled by emergency recovery");
        }
        windows.set_visible(hwnd, false);
        // Recovery may have shown this window while ShowWindow(SW_HIDE) was still
        // running. Repair AFTER that effect, including previously hidden windows.
        if recovered.load(Ordering::SeqCst) {
            restore_records(state, windows, true, true)?;
            state.hidden_by_dockwind = false;
            bail!("taskbar hide cancelled by emergency recovery");
        }
        if windows.is_visible(hwnd) {
            // The records describe windows this call has just hidden, so treat them
            // as hidden even though the ownership flag is only set after the loop.
            let cleanup = restore_records(state, windows, false, true);
            if let Err(cleanup_error) = cleanup {
                warn!(%cleanup_error, "failed to restore the taskbar after a partial hide");
            }
            return Err(anyhow!(
                "failed to hide the native taskbar window 0x{hwnd:X}"
            ));
        }
    }

    state.hidden_by_dockwind = true;
    info!(count = state.records.len(), "native taskbar hidden");
    Ok(())
}

fn restore_locked(
    state: &mut TaskbarState,
    windows: &dyn TaskbarWindows,
    force: bool,
) -> Result<()> {
    if !state.hidden_by_dockwind && !force {
        return Ok(());
    }

    let handles = windows.handles()?;
    if handles.is_empty() {
        bail!("no native taskbar windows were found while restoring");
    }

    if force {
        state.records.clear();
        state
            .records
            .extend(handles.into_iter().map(|hwnd| TaskbarRecord {
                hwnd,
                was_visible: true,
            }));
    }

    restore_records(state, windows, force, state.hidden_by_dockwind || force)?;
    state.hidden_by_dockwind = false;
    info!("native taskbar restored");
    Ok(())
}

fn restore_records(
    state: &mut TaskbarState,
    windows: &dyn TaskbarWindows,
    force: bool,
    hidden_by_dockwind: bool,
) -> Result<()> {
    let mut failures = Vec::new();

    for record in state.records.iter() {
        if !needs_show(force, hidden_by_dockwind, record.was_visible) {
            continue;
        }

        windows.set_visible(record.hwnd, true);
        if !windows.is_visible(record.hwnd) {
            failures.push(record.hwnd);
        }
    }

    if failures.is_empty() {
        state.records.clear();
        return Ok(());
    }

    // Keep the records so a later drop or recovery command can retry.
    state.hidden_by_dockwind = true;
    Err(anyhow!(
        "failed to restore {} native taskbar window(s)",
        failures.len()
    ))
}

/// Called by the watcher for `TaskbarCreated`. The ownership check and the re-hide
/// run under the same lock as `restore_native`, so a restore that wins the lock is
/// never followed by a hide from this callback.
fn hide_after_taskbar_created() {
    if let Err(error) = TASKBAR_OWNERSHIP.hide(&NativeTaskbar, true) {
        warn!(%error, "failed to hide the restarted taskbar");
    }
}

/// Emergency path used by panic and console handlers. It never takes the state lock,
/// so it cannot deadlock if the panic interrupted code holding that lock. It may show
/// a taskbar the user hid intentionally; that is preferable to leaving the desktop
/// without one during an abnormal exit.
pub(super) fn force_show_taskbars() -> Result<()> {
    TASKBAR_OWNERSHIP.recover(&NativeTaskbar)
}

fn taskbar_handles() -> Result<Vec<HWND>> {
    let mut handles = Vec::new();
    // SAFETY: the callback only appends to the vector behind `lparam`. EnumWindows
    // returns false when enumeration fails or the callback stops it; this callback
    // always continues, so false means an error.
    unsafe {
        EnumWindows(
            Some(enum_taskbar_proc),
            LPARAM(&mut handles as *mut Vec<HWND> as isize),
        )
    }
    .context("EnumWindows failed")?;
    Ok(handles)
}

/// SAFETY: Windows calls this with `lparam` set to the pointer passed to EnumWindows,
/// which points to a live `Vec<HWND>` for the duration of the enumeration.
unsafe extern "system" fn enum_taskbar_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: the caller guarantees `lparam` is the live vector pointer from EnumWindows.
    let handles = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    let mut class_name = [0u16; 256];
    // SAFETY: the buffer is valid for 256 UTF-16 code units.
    let length = unsafe { GetClassNameW(hwnd, &mut class_name) };
    if length > 0 {
        let class = String::from_utf16_lossy(&class_name[..length as usize]);
        if class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd" {
            handles.push(hwnd);
        }
    }
    BOOL(1)
}

/// SAFETY: `hwnd` is the watcher window. The handler only performs Win32 calls and
/// logging; it never unwinds and never dereferences user pointers.
unsafe extern "system" fn taskbar_watch_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_ENDSESSION && wparam.0 != 0 {
        if let Err(error) = restore_native(false) {
            warn!(%error, "failed to restore the taskbar on session end");
        }
    } else {
        let created = *TASKBAR_CREATED_MESSAGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if created.is_some_and(|created| message == created) {
            hide_after_taskbar_created();
        }
    }

    // SAFETY: default handling for every message this window does not own.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// SAFETY: console control handlers run on a system thread. The handler only uses the
/// lock-free force path and logging; it never unwinds. Returning FALSE lets other
/// handlers run.
unsafe extern "system" fn console_handler(_control_type: u32) -> BOOL {
    if let Err(error) = force_show_taskbars() {
        warn!(%error, "failed to restore the taskbar from the console handler");
    }
    BOOL(0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    struct FakeTaskbar {
        visible: Mutex<Vec<(isize, bool)>>,
        actions: Mutex<Vec<(isize, bool)>>,
    }

    impl FakeTaskbar {
        fn new(handles: &[isize]) -> Self {
            Self {
                visible: Mutex::new(handles.iter().map(|hwnd| (*hwnd, true)).collect()),
                actions: Mutex::new(Vec::new()),
            }
        }

        fn visible(&self, hwnd: isize) -> bool {
            self.visible
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .find(|(handle, _)| *handle == hwnd)
                .is_some_and(|(_, visible)| *visible)
        }

        fn last_action(&self) -> Option<(isize, bool)> {
            self.actions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .last()
                .copied()
        }
    }

    impl TaskbarWindows for FakeTaskbar {
        fn handles(&self) -> Result<Vec<isize>> {
            Ok(self
                .visible
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .map(|(hwnd, _)| *hwnd)
                .collect())
        }

        fn is_visible(&self, hwnd: isize) -> bool {
            self.visible(hwnd)
        }

        fn set_visible(&self, hwnd: isize, visible: bool) {
            {
                let mut state = self
                    .visible
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(entry) = state.iter_mut().find(|(handle, _)| *handle == hwnd) {
                    entry.1 = visible;
                }
            }
            self.actions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push((hwnd, visible));
        }
    }

    #[test]
    fn restore_visibility_follows_the_recorded_state() {
        assert!(needs_show(true, false, false));
        assert!(needs_show(false, true, true));
        assert!(!needs_show(false, false, true));
        assert!(!needs_show(false, true, false));
    }

    // Pause a real policy transition at the native visibility-effect boundary.
    // No global production ownership state or desktop taskbar is used by these tests.
    struct PausedTaskbar {
        taskbar: FakeTaskbar,
        pause_visible: bool,
        pause_handle: isize,
        pause_once: AtomicBool,
        entered: mpsc::Sender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
    }

    impl TaskbarWindows for PausedTaskbar {
        fn handles(&self) -> Result<Vec<isize>> {
            self.taskbar.handles()
        }

        fn is_visible(&self, hwnd: isize) -> bool {
            self.taskbar.is_visible(hwnd)
        }

        fn set_visible(&self, hwnd: isize, visible: bool) {
            if visible == self.pause_visible
                && hwnd == self.pause_handle
                && self.pause_once.swap(false, Ordering::SeqCst)
            {
                self.entered.send(()).expect("effect entered");
                self.resume
                    .lock()
                    .expect("resume lock")
                    .recv_timeout(Duration::from_secs(5))
                    .expect("resume effect");
            }
            self.taskbar.set_visible(hwnd, visible);
        }
    }

    fn paused_taskbar(
        visible: bool,
        pause_handle: isize,
    ) -> (Arc<PausedTaskbar>, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered, entered_rx) = mpsc::channel();
        let (resume_tx, resume) = mpsc::channel();
        (
            Arc::new(PausedTaskbar {
                taskbar: FakeTaskbar::new(&[1, 2]),
                pause_visible: visible,
                pause_handle,
                pause_once: AtomicBool::new(true),
                entered,
                resume: Mutex::new(resume),
            }),
            entered_rx,
            resume_tx,
        )
    }

    #[test]
    fn restore_waits_for_in_flight_callback_and_remains_visible() {
        let ownership = Arc::new(TaskbarOwnership::new());
        let (taskbar, entered, resume) = paused_taskbar(false, 1);
        // Establish ownership through the same transition used by the guard.
        taskbar.pause_once.store(false, Ordering::SeqCst);
        ownership
            .hide(taskbar.as_ref(), false)
            .expect("initial hide");
        taskbar.pause_once.store(true, Ordering::SeqCst);
        // Explorer's replacement taskbars start visible.
        taskbar.taskbar.set_visible(1, true);
        taskbar.taskbar.set_visible(2, true);
        let rehide = {
            let ownership = Arc::clone(&ownership);
            let taskbar = Arc::clone(&taskbar);
            std::thread::spawn(move || ownership.hide(taskbar.as_ref(), true))
        };
        entered
            .recv_timeout(Duration::from_secs(5))
            .expect("hide paused");
        assert!(
            ownership.state.try_lock().is_err(),
            "hide effects must own the state lock"
        );
        let restore = {
            let ownership = Arc::clone(&ownership);
            let taskbar = Arc::clone(&taskbar);
            std::thread::spawn(move || ownership.restore(taskbar.as_ref(), false))
        };
        resume.send(()).expect("resume hide");
        rehide.join().expect("rehide thread").expect("rehide");
        restore.join().expect("restore thread").expect("restore");
        ownership
            .hide(taskbar.as_ref(), true)
            .expect("late callback");
        assert!(taskbar.is_visible(1) && taskbar.is_visible(2));
        assert!(!ownership.state.lock().expect("state").hidden_by_dockwind);
    }

    #[test]
    fn callback_waits_for_restore_and_does_not_rehide() {
        let ownership = Arc::new(TaskbarOwnership::new());
        let (taskbar, entered, resume) = paused_taskbar(true, 1);
        ownership
            .hide(taskbar.as_ref(), false)
            .expect("initial hide");
        let restore = {
            let ownership = Arc::clone(&ownership);
            let taskbar = Arc::clone(&taskbar);
            std::thread::spawn(move || ownership.restore(taskbar.as_ref(), false))
        };
        entered
            .recv_timeout(Duration::from_secs(5))
            .expect("restore paused");
        assert!(
            ownership.state.try_lock().is_err(),
            "restore effects must own the state lock"
        );
        let (started_tx, started_rx) = mpsc::channel();
        let rehide = {
            let ownership = Arc::clone(&ownership);
            let taskbar = Arc::clone(&taskbar);
            std::thread::spawn(move || {
                started_tx.send(()).expect("callback started");
                ownership.hide(taskbar.as_ref(), true)
            })
        };
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("callback started");
        resume.send(()).expect("resume restore");
        restore.join().expect("restore thread").expect("restore");
        rehide.join().expect("callback thread").expect("callback");
        assert!(taskbar.is_visible(1) && taskbar.is_visible(2));
        assert_eq!(taskbar.taskbar.last_action(), Some((2, true)));
    }

    #[test]
    fn emergency_recovery_repairs_hide_already_inside_native_effect() {
        let ownership = Arc::new(TaskbarOwnership::new());
        let (taskbar, entered, resume) = paused_taskbar(false, 2);
        let hide = {
            let ownership = Arc::clone(&ownership);
            let taskbar = Arc::clone(&taskbar);
            std::thread::spawn(move || ownership.hide(taskbar.as_ref(), false))
        };
        entered
            .recv_timeout(Duration::from_secs(5))
            .expect("hide paused");
        assert!(ownership.state.try_lock().is_err());
        ownership
            .recover(taskbar.as_ref())
            .expect("lock-free recovery");
        resume
            .send(())
            .expect("complete native hide after recovery");
        let error = hide
            .join()
            .expect("hide thread")
            .expect_err("hide cancelled");
        assert!(
            error
                .to_string()
                .contains("cancelled by emergency recovery")
        );
        ownership
            .hide(taskbar.as_ref(), true)
            .expect("late callback");
        assert!(
            ownership.hide(taskbar.as_ref(), false).is_err(),
            "latch must not reset"
        );
        assert!(taskbar.is_visible(1) && taskbar.is_visible(2));
        assert!(!ownership.state.lock().expect("state").hidden_by_dockwind);
    }

    #[test]
    fn emergency_recovery_invalidates_completed_hidden_ownership() {
        let ownership = TaskbarOwnership::new();
        let taskbar = FakeTaskbar::new(&[1, 2]);
        ownership.hide(&taskbar, false).expect("initial hide");
        ownership.recover(&taskbar).expect("recover");
        ownership.hide(&taskbar, true).expect("late callback");
        assert!(taskbar.is_visible(1) && taskbar.is_visible(2));
        assert_eq!(taskbar.last_action(), Some((2, true)));
    }

    #[test]
    fn watcher_can_be_restarted_and_rejects_a_second_instance() {
        let mut first = install_watch().expect("first taskbar watcher");
        assert!(
            install_watch().is_err(),
            "a second watcher must fail while the class is registered"
        );
        first.shutdown().expect("stop the first watcher");
        first.shutdown().expect("shutdown is idempotent");

        let mut second = install_watch().expect("restarted taskbar watcher");
        second.shutdown().expect("stop the restarted watcher");
    }
}
