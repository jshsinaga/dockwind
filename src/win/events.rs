use std::sync::{Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use tracing::{error, warn};
use windows::Win32::UI::{
    Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
    WindowsAndMessaging::{
        DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE,
        EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
        EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, OBJID_WINDOW, TranslateMessage,
        WINEVENT_OUTOFCONTEXT,
    },
};

use crate::win::worker;

const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Registered by the running event thread so the out-of-context hook callback can
/// forward events. `None` while no worker is running.
static EVENT_SENDER: Mutex<Option<UnboundedSender<()>>> = Mutex::new(None);

/// Owns the window event thread. Call [`EventWorker::shutdown`] before the process
/// exits so the hook is removed and the thread is joined.
pub struct EventWorker {
    stop: Option<isize>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl EventWorker {
    /// Signals the stop event, waits for the hook cleanup, and joins the thread.
    /// Safe to call more than once.
    pub fn shutdown(&mut self) -> Result<()> {
        worker::shutdown(&mut self.stop, &mut self.thread, worker::signal_stop)
    }
}

impl Drop for EventWorker {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            worker::finish_failed_shutdown(&mut self.stop, &mut self.thread, &error);
            warn!(%error, "failed to stop the window event worker on drop");
        }
    }
}

/// Starts the dedicated window event thread and returns its receiver.
pub fn spawn() -> Result<(EventWorker, UnboundedReceiver<()>)> {
    let (sender, receiver) = unbounded();
    {
        let mut slot = EVENT_SENDER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            bail!("window event hook is already running");
        }
        *slot = Some(sender);
    }

    let stop = match worker::create_stop_signal() {
        Ok(stop) => stop,
        Err(error) => {
            clear_sender();
            return Err(error);
        }
    };

    let (ready_sender, ready_receiver) = mpsc::channel();
    let thread = match std::thread::Builder::new()
        .name("win-events".to_string())
        .spawn(move || run(stop, ready_sender))
    {
        Ok(thread) => thread,
        Err(error) => {
            clear_sender();
            if let Err(cleanup_error) = worker::close_stop_signal(stop) {
                warn!(%cleanup_error, "failed to close the window event stop signal");
            }
            return Err(anyhow!("failed to start window event thread: {error}"));
        }
    };

    let mut event_worker = EventWorker {
        stop: Some(stop),
        thread: Some(thread),
    };

    match ready_receiver.recv_timeout(READY_TIMEOUT) {
        Ok(Ok(())) => Ok((event_worker, receiver)),
        Ok(Err(message)) => {
            if let Err(cleanup_error) = event_worker.shutdown() {
                warn!(%cleanup_error, "failed to clean up the window event thread");
            }
            Err(anyhow!(message))
        }
        Err(error) => {
            if let Err(cleanup_error) = event_worker.shutdown() {
                warn!(%cleanup_error, "failed to clean up the window event thread");
            }
            Err(anyhow!("window event thread did not become ready: {error}"))
        }
    }
}

struct SenderRegistration;

impl Drop for SenderRegistration {
    fn drop(&mut self) {
        clear_sender();
    }
}

fn clear_sender() {
    let mut slot = EVENT_SENDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = None;
}

fn run(stop: isize, ready_sender: mpsc::Sender<Result<(), String>>) -> Result<()> {
    let _registration = SenderRegistration;

    // SAFETY: SetWinEventHook only registers an out-of-context callback. The hook
    // range covers the events filtered again in `callback`. A null hook means the
    // call failed and is reported below.
    let object_hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_NAMECHANGE,
            None,
            Some(callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if object_hook.0.is_null() {
        let message = format!(
            "SetWinEventHook for object events failed: {}",
            std::io::Error::last_os_error()
        );
        let _ = ready_sender.send(Err(message.clone()));
        return Err(anyhow!(message));
    }

    // SAFETY: same contract as the object hook above. On failure the object hook is
    // unhooked before this thread exits.
    let system_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_MINIMIZEEND,
            None,
            Some(callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if system_hook.0.is_null() {
        unhook(object_hook);
        let message = format!(
            "SetWinEventHook for system events failed: {}",
            std::io::Error::last_os_error()
        );
        let _ = ready_sender.send(Err(message.clone()));
        return Err(anyhow!(message));
    }

    let _ = ready_sender.send(Ok(()));

    let loop_result = message_loop(stop);
    if let Err(error) = &loop_result {
        error!(%error, "window event message loop stopped");
    }

    unhook(object_hook);
    unhook(system_hook);

    loop_result
}

fn unhook(hook: HWINEVENTHOOK) {
    // SAFETY: the only callers pass a hook that SetWinEventHook created on this
    // thread. It is removed exactly once.
    if !unsafe { UnhookWinEvent(hook) }.as_bool() {
        warn!("failed to remove a window event hook");
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

/// Out-of-context WinEvent callback.
///
/// SAFETY: Windows calls this on an arbitrary thread with an `HWND` that may already
/// be invalid. The callback only compares integers and forwards a unit value; it does
/// not dereference the handle, allocate, or unwind.
unsafe extern "system" fn callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.0.is_null() || id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }

    if !matches!(
        event,
        EVENT_OBJECT_CREATE
            | EVENT_OBJECT_SHOW
            | EVENT_OBJECT_HIDE
            | EVENT_OBJECT_DESTROY
            | EVENT_OBJECT_NAMECHANGE
            | EVENT_SYSTEM_FOREGROUND
            | EVENT_SYSTEM_MINIMIZESTART
            | EVENT_SYSTEM_MINIMIZEEND
    ) {
        return;
    }

    // A poisoned lock cannot make the sender invalid; recover the guard instead of
    // panicking in the callback.
    let slot = EVENT_SENDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(sender) = slot.as_ref() {
        let _ = sender.unbounded_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_can_be_restarted_after_shutdown() {
        let (mut first, _first_receiver) = spawn().expect("first window event worker");
        assert!(
            spawn().is_err(),
            "a second worker must fail while the first is running"
        );

        first.shutdown().expect("stop the first worker");
        first.shutdown().expect("shutdown is idempotent");

        let (mut second, _second_receiver) = spawn().expect("restarted window event worker");
        second.shutdown().expect("stop the restarted worker");
    }
}
