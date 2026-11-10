use std::ffi::c_void;
use std::thread::JoinHandle;

use anyhow::{Context as _, Result, anyhow};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForSingleObject},
    UI::WindowsAndMessaging::{
        MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
        QS_ALLINPUT, WM_QUIT,
    },
};

/// Why a worker thread woke up from its wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    Stop,
    Message,
}

/// Creates an owned manual-reset stop event used to terminate a worker thread.
pub fn create_stop_signal() -> Result<isize> {
    // SAFETY: no security attributes or name are used. A manual-reset event starts
    // unsignaled, and a failure returns an error instead of a handle.
    let handle = unsafe { CreateEventW(None, true, false, None) }.context("CreateEventW failed")?;
    Ok(handle.0 as isize)
}

/// Signals a worker to stop. This works even when its message queue is busy, so it
/// does not depend on `PostThreadMessageW`.
pub fn signal_stop(stop: isize) -> Result<()> {
    let handle = HANDLE(stop as *mut c_void);
    // SAFETY: `stop` comes from `create_stop_signal` and is closed only after the
    // worker thread has joined.
    unsafe { SetEvent(handle) }.context("SetEvent failed")
}

/// Closes a stop event after its worker thread has joined.
pub fn close_stop_signal(stop: isize) -> Result<()> {
    let handle = HANDLE(stop as *mut c_void);
    // SAFETY: the event is owned by the worker and is closed exactly once, after the
    // waiting thread has joined.
    unsafe { CloseHandle(handle) }.context("CloseHandle(stop signal) failed")
}

/// Signals before consuming ownership. A signaling failure leaves both handles
/// available for retry and never joins a worker that has not been told to stop.
pub fn shutdown(
    stop: &mut Option<isize>,
    thread: &mut Option<JoinHandle<Result<()>>>,
    signal: impl FnOnce(isize) -> Result<()>,
) -> Result<()> {
    if thread.as_ref().is_some_and(|thread| !thread.is_finished()) {
        signal(stop.context("worker has no stop signal")?)?;
    }

    let joined = match thread.take() {
        Some(thread) => thread
            .join()
            .map_err(|_| anyhow!("worker thread panicked"))
            .and_then(|result| result),
        None => Ok(()),
    };
    let closed = match *stop {
        Some(handle) => close_stop_signal(handle).map(|()| *stop = None),
        None => Ok(()),
    };
    if let Err(error) = joined {
        if let Err(cleanup_error) = closed {
            tracing::warn!(%cleanup_error, "failed to close the worker stop signal");
        }
        return Err(error);
    }
    closed
}

/// A destructor cannot return a live, uncancelled worker to its caller. Abort only
/// in that terminal case, after recovery and a synchronous diagnostic. Completed
/// workers and ordinary cleanup failures can still be reported without aborting.
pub fn finish_failed_shutdown(
    stop: &mut Option<isize>,
    thread: &mut Option<JoinHandle<Result<()>>>,
    error: &anyhow::Error,
) {
    if thread.as_ref().is_some_and(|thread| !thread.is_finished()) {
        let recovery = super::taskbar::force_show_taskbars();
        // Do not depend on the asynchronous logger being flushed before abort.
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr().lock(),
            "fatal: cannot cancel a live native worker: {error:#}; taskbar recovery: {recovery:?}"
        );
        if thread.as_ref().is_some_and(|thread| !thread.is_finished()) {
            std::process::abort();
        }
    }
    // A worker can finish between the failed signal and this destructor check.
    // Join it and close the event instead of detaching a completed thread.
    if thread.is_some()
        && let Err(cleanup_error) = shutdown(stop, thread, signal_stop)
    {
        tracing::warn!(%cleanup_error, "failed to release a completed native worker");
    }
}

/// Returns one posted message, or `None` for cancellation or WM_QUIT. A sent-only
/// wake can leave no posted message; PeekMessage returns without blocking then.
/// Returning to the wait after each message gives stop priority over a busy queue.
pub fn next_message(stop: isize) -> Result<Option<MSG>> {
    loop {
        if wait_for_stop_or_message(stop)? == Wake::Stop {
            return Ok(None);
        }
        let mut message = MSG::default();
        // SAFETY: `message` is writable and no window or message filter is used.
        // PeekMessage also dispatches sent messages on the current worker thread.
        if unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            return Ok((message.message != WM_QUIT).then_some(message));
        }
    }
}

/// Waits for either the stop event or queued thread messages.
pub fn wait_for_stop_or_message(stop: isize) -> Result<Wake> {
    let handle = HANDLE(stop as *mut c_void);
    // Check the event explicitly: MWMO_INPUTAVAILABLE may report a ready queue
    // before checking handles. A busy queue must not starve cancellation.
    // SAFETY: `stop` remains live until the owning worker has joined; zero never waits.
    match unsafe { WaitForSingleObject(handle, 0) } {
        WAIT_OBJECT_0 => return Ok(Wake::Stop),
        WAIT_TIMEOUT => {}
        result => {
            return Err(anyhow!(
                "stop event check failed (0x{:X}): {}",
                result.0,
                std::io::Error::last_os_error()
            ));
        }
    }
    // SAFETY: `stop` is a live event owned by the worker thread. `QS_ALLINPUT` wakes
    // the wait when any queued message arrives.
    let result = unsafe {
        MsgWaitForMultipleObjectsEx(Some(&[handle]), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
    };

    if result == WAIT_OBJECT_0 {
        return Ok(Wake::Stop);
    }
    if result == WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
        return Ok(Wake::Message);
    }
    if result == WAIT_FAILED {
        return Err(anyhow!(
            "MsgWaitForMultipleObjectsEx failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Err(anyhow!(
        "MsgWaitForMultipleObjectsEx returned an unexpected value 0x{:X}",
        result.0
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            System::Threading::GetCurrentThreadId,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, HWND_MESSAGE, PM_NOREMOVE, PostThreadMessageW,
                SMTO_ABORTIFHUNG, SendMessageTimeoutW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
                WM_NULL,
            },
        },
        core::w,
    };

    use super::*;

    #[test]
    fn stop_signal_wakes_the_wait_without_a_message() {
        let stop = create_stop_signal().expect("stop signal");
        signal_stop(stop).expect("signal stop");
        assert_eq!(
            wait_for_stop_or_message(stop).expect("wait"),
            Wake::Stop,
            "an owned stop event must end the wait even with an empty message queue"
        );
        close_stop_signal(stop).expect("close stop signal");
    }

    #[test]
    fn signaling_failure_preserves_ownership_without_joining_and_can_retry() {
        let handle = create_stop_signal().expect("stop signal");
        let (done_tx, done_rx) = mpsc::channel();
        let mut thread = Some(std::thread::spawn(move || {
            let wake = wait_for_stop_or_message(handle)?;
            assert_eq!(wake, Wake::Stop);
            done_tx.send(()).expect("completion");
            Ok(())
        }));
        let mut stop = Some(handle);
        let error = shutdown(&mut stop, &mut thread, |_| {
            Err(anyhow!("injected SetEvent failure"))
        })
        .expect_err("signal failure");
        assert_eq!(error.to_string(), "injected SetEvent failure");
        assert!(
            thread.is_some(),
            "failed cancellation must retain the join handle"
        );
        assert_eq!(stop, Some(handle));
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        shutdown(&mut stop, &mut thread, signal_stop).expect("retry shutdown");
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker stopped");
        assert!(stop.is_none() && thread.is_none());
        shutdown(&mut stop, &mut thread, signal_stop).expect("idempotent shutdown");
    }

    #[test]
    fn completed_worker_is_joined_without_signaling_and_preserves_its_error() {
        let mut stop = Some(create_stop_signal().expect("stop signal"));
        let handle = std::thread::spawn(|| Err(anyhow!("worker failed")));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !handle.is_finished() {
            assert!(std::time::Instant::now() < deadline, "worker must finish");
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut thread = Some(handle);
        let error = shutdown(&mut stop, &mut thread, |_| {
            panic!("must not signal completed worker")
        })
        .expect_err("worker error");
        assert_eq!(error.to_string(), "worker failed");
        assert!(stop.is_none() && thread.is_none());
    }

    #[test]
    fn stop_takes_priority_while_messages_remain_queued() {
        std::thread::spawn(|| {
            let stop = create_stop_signal().expect("stop signal");
            let mut message = MSG::default();
            // SAFETY: this initializes this test thread's own message queue.
            let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
            // SAFETY: the thread ID identifies this live thread with an initialized queue.
            let id = unsafe { GetCurrentThreadId() };
            // SAFETY: posts scalar messages to the test thread's own queue.
            unsafe {
                PostThreadMessageW(id, WM_APP, WPARAM(0), LPARAM(0)).expect("first message");
                PostThreadMessageW(id, WM_APP, WPARAM(0), LPARAM(0)).expect("second message");
            }
            assert_eq!(
                next_message(stop)
                    .expect("first wake")
                    .expect("message")
                    .message,
                WM_APP
            );
            signal_stop(stop).expect("signal while queue is nonempty");
            assert!(next_message(stop).expect("stop wake").is_none());
            close_stop_signal(stop).expect("close stop");
        })
        .join()
        .expect("message thread");
    }

    #[test]
    fn quit_ends_the_reader_without_returning_a_dispatchable_message() {
        std::thread::spawn(|| {
            let stop = create_stop_signal().expect("stop signal");
            let mut message = MSG::default();
            // SAFETY: creates the current thread's queue using valid writable storage.
            let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
            // SAFETY: current thread is live and its queue was initialized above.
            unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_QUIT, WPARAM(0), LPARAM(0)) }
                .expect("post quit");
            assert!(next_message(stop).expect("quit wake").is_none());
            close_stop_signal(stop).expect("close stop");
        })
        .join()
        .expect("quit thread");
    }

    #[test]
    fn sent_only_wake_returns_to_cancellable_wait() {
        let stop = create_stop_signal().expect("stop signal");
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            // SAFETY: STATIC is a system class. The window is message-only and owned
            // and destroyed by this test thread; no desktop taskbar is accessed.
            let window = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!(""),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            }
            .expect("message-only window");
            // SAFETY: retrieves the current test worker's live thread ID.
            ready_tx
                .send((window.0 as isize, unsafe { GetCurrentThreadId() }))
                .expect("ready");
            let result = next_message(stop).map(|message| message.is_none());
            // SAFETY: window was created on this thread and has not been destroyed.
            unsafe { DestroyWindow(window) }.expect("destroy test window");
            done_tx.send(result).expect("done");
        });
        let (window, id) = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("ready window");
        // SAFETY: the window belongs to our live test worker; only a scalar WM_NULL
        // is sent. A timeout bounds failures if the worker does not pump sent input.
        let sent = unsafe {
            SendMessageTimeoutW(
                HWND(window as *mut c_void),
                WM_NULL,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                2000,
                None,
            )
        };
        signal_stop(stop).expect("signal after sent message");
        let result = done_rx.recv_timeout(Duration::from_secs(2));
        if result.is_err() {
            // SAFETY: regression cleanup unblocks an old GetMessage implementation
            // before asserting failure, so the test does not leave a stuck worker.
            unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) }
                .expect("release regressed message loop");
        }
        thread.join().expect("message worker");
        close_stop_signal(stop).expect("close stop");
        assert_ne!(sent.0, 0, "sent message must have been dispatched");
        assert!(
            result
                .expect("stop must wake without any posted message")
                .expect("reader result")
        );
    }
}
