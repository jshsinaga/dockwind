use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            DispatchMessageW, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_NAMECHANGE,
            EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND,
            EVENT_SYSTEM_MINIMIZESTART, GetMessageW, MSG, OBJID_WINDOW, TranslateMessage,
            WINEVENT_OUTOFCONTEXT,
        },
    },
};

static EVENT_SENDER: OnceLock<UnboundedSender<()>> = OnceLock::new();

pub fn spawn() -> Result<UnboundedReceiver<()>> {
    let (sender, receiver) = unbounded();
    EVENT_SENDER
        .set(sender)
        .map_err(|_| anyhow!("window event hook already started"))?;

    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("win-events".to_string())
        .spawn(move || unsafe {
            let object_hook = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_NAMECHANGE,
                None,
                Some(callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if object_hook.0.is_null() {
                let _ = ready_sender.send(Err(anyhow!("SetWinEventHook for object events failed")));
                return;
            }

            let system_hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_MINIMIZEEND,
                None,
                Some(callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if system_hook.0.is_null() {
                let _ = UnhookWinEvent(object_hook);
                let _ = ready_sender.send(Err(anyhow!("SetWinEventHook for system events failed")));
                return;
            }

            let _ = ready_sender.send(Ok(()));

            let mut message = MSG::default();
            loop {
                let result = GetMessageW(&mut message, None, 0, 0);
                if result.0 <= 0 {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            let _ = UnhookWinEvent(object_hook);
            let _ = UnhookWinEvent(system_hook);
        })
        .map_err(|error| anyhow!("failed to start window event thread: {error}"))?;

    match ready_receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(receiver),
        Ok(Err(error)) => Err(error),
        Err(error) => bail!("window event thread did not start: {error}"),
    }
}

unsafe extern "system" fn callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
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
        EVENT_OBJECT_SHOW
            | EVENT_OBJECT_HIDE
            | EVENT_OBJECT_DESTROY
            | EVENT_OBJECT_NAMECHANGE
            | EVENT_SYSTEM_FOREGROUND
            | EVENT_SYSTEM_MINIMIZESTART
            | EVENT_SYSTEM_MINIMIZEEND
    ) {
        return;
    }

    if let Some(sender) = EVENT_SENDER.get() {
        let _ = sender.unbounded_send(());
    }
}
