use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Result, anyhow};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use tracing::warn;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey,
        UnregisterHotKey,
    },
    WindowsAndMessaging::WM_HOTKEY,
};

use crate::win::worker;

const HOTKEY_ID: i32 = 1;
const READY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hotkey {
    modifiers: HOT_KEY_MODIFIERS,
    vk: u32,
}

/// Owns the global hotkey thread. Call [`HotkeyWorker::shutdown`] to stop the thread,
/// unregister the hotkey, and join it.
pub struct HotkeyWorker {
    stop: Option<isize>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl HotkeyWorker {
    /// Signals the stop event and joins the worker. Safe to call more than once and
    /// after the worker already exited on `WM_HOTKEY`.
    pub fn shutdown(&mut self) -> Result<()> {
        worker::shutdown(&mut self.stop, &mut self.thread, worker::signal_stop)
    }
}

impl Drop for HotkeyWorker {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            worker::finish_failed_shutdown(&mut self.stop, &mut self.thread, &error);
            warn!(%error, "failed to stop the hotkey worker on drop");
        }
    }
}

/// Registers the global quit hotkey on a dedicated message-loop thread.
pub fn spawn(value: &str) -> Result<(HotkeyWorker, UnboundedReceiver<()>)> {
    let hotkey = parse_hotkey(value).map_err(|error| anyhow!("invalid quit_hotkey: {error}"))?;
    let (sender, receiver) = unbounded();
    let stop = worker::create_stop_signal()?;
    let (ready_sender, ready_receiver) = mpsc::channel();

    let thread = match std::thread::Builder::new()
        .name("quit-hotkey".to_string())
        .spawn(move || run(stop, hotkey, sender, ready_sender))
    {
        Ok(thread) => thread,
        Err(error) => {
            if let Err(cleanup_error) = worker::close_stop_signal(stop) {
                warn!(%cleanup_error, "failed to close the hotkey stop signal");
            }
            return Err(anyhow!("failed to start hotkey thread: {error}"));
        }
    };

    let mut hotkey_worker = HotkeyWorker {
        stop: Some(stop),
        thread: Some(thread),
    };

    match ready_receiver.recv_timeout(READY_TIMEOUT) {
        Ok(Ok(())) => Ok((hotkey_worker, receiver)),
        Ok(Err(message)) => {
            if let Err(cleanup_error) = hotkey_worker.shutdown() {
                warn!(%cleanup_error, "failed to clean up the hotkey thread");
            }
            Err(anyhow!(message))
        }
        Err(error) => {
            if let Err(cleanup_error) = hotkey_worker.shutdown() {
                warn!(%cleanup_error, "failed to clean up the hotkey thread");
            }
            Err(anyhow!("hotkey thread did not become ready: {error}"))
        }
    }
}

fn run(
    stop: isize,
    hotkey: Hotkey,
    sender: UnboundedSender<()>,
    ready_sender: mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    // SAFETY: RegisterHotKey with a null window associates the hotkey with the calling
    // thread's message queue. The modifiers and virtual key were parsed and validated.
    if let Err(error) = unsafe { RegisterHotKey(None, HOTKEY_ID, hotkey.modifiers, hotkey.vk) } {
        let message = format!("RegisterHotKey failed: {error}");
        let _ = ready_sender.send(Err(message.clone()));
        return Err(anyhow!(message));
    }

    let _ = ready_sender.send(Ok(()));

    let loop_result = message_loop(stop, &sender);

    // SAFETY: RegisterHotKey succeeded above on this thread, so unregistering the same
    // id on this thread is valid exactly once.
    let unregister = unsafe { UnregisterHotKey(None, HOTKEY_ID) };

    match loop_result {
        Ok(()) => unregister.map_err(|error| anyhow!("UnregisterHotKey failed: {error}")),
        Err(error) => {
            if let Err(cleanup_error) = unregister {
                warn!(%cleanup_error, "failed to unregister the quit hotkey");
            }
            Err(error)
        }
    }
}

fn message_loop(stop: isize, sender: &UnboundedSender<()>) -> Result<()> {
    while let Some(message) = worker::next_message(stop)? {
        if message.message == WM_HOTKEY {
            // A closed consumer means the application is already shutting down.
            let _ = sender.unbounded_send(());
            return Ok(());
        }
    }
    Ok(())
}

fn parse_hotkey(value: &str) -> Result<Hotkey, String> {
    let mut modifiers = 0u32;
    let mut virtual_key = None;

    for part in value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
            "alt" => modifiers |= MOD_ALT.0,
            "shift" => modifiers |= MOD_SHIFT.0,
            "win" | "super" | "meta" => modifiers |= MOD_WIN.0,
            key => {
                if virtual_key.is_some() {
                    return Err(format!("multiple keys in '{value}'"));
                }
                virtual_key = Some(parse_key(key)?);
            }
        }
    }

    if modifiers == 0 {
        return Err(format!("hotkey '{value}' has no modifier"));
    }
    let Some(vk) = virtual_key else {
        return Err(format!("hotkey '{value}' has no key"));
    };

    Ok(Hotkey {
        modifiers: HOT_KEY_MODIFIERS(modifiers),
        vk,
    })
}

fn parse_key(key: &str) -> Result<u32, String> {
    let key = key.to_ascii_uppercase();
    let bytes = key.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Ok(bytes[0] as u32);
    }

    match key.as_str() {
        "SPACE" => Ok(0x20),
        "TAB" => Ok(0x09),
        "ESC" | "ESCAPE" => Ok(0x1B),
        "ENTER" | "RETURN" => Ok(0x0D),
        "BACKSPACE" => Ok(0x08),
        "DELETE" | "DEL" => Ok(0x2E),
        "INSERT" | "INS" => Ok(0x2D),
        "HOME" => Ok(0x24),
        "END" => Ok(0x23),
        "PAGEUP" | "PGUP" => Ok(0x21),
        "PAGEDOWN" | "PGDN" => Ok(0x22),
        _ => {
            if let Some(number) = key.strip_prefix('F')
                && let Ok(number) = number.parse::<u8>()
                && (1..=12).contains(&number)
            {
                return Ok(0x70 + number as u32 - 1);
            }
            Err(format!("unknown key '{key}'"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_quit_hotkey() {
        let hotkey = parse_hotkey("Ctrl+Alt+Shift+Q").expect("valid hotkey");
        assert_eq!(hotkey.modifiers.0, MOD_CONTROL.0 | MOD_ALT.0 | MOD_SHIFT.0);
        assert_eq!(hotkey.vk, b'Q' as u32);
    }

    #[test]
    fn parses_function_keys_and_named_keys() {
        assert_eq!(parse_key("F12").expect("F12"), 0x7B);
        assert_eq!(parse_key("space").expect("space"), 0x20);
        assert_eq!(parse_key("Esc").expect("esc"), 0x1B);
    }

    #[test]
    fn rejects_incomplete_or_unknown_hotkeys() {
        for value in [
            "",
            "Q",
            "Ctrl+",
            "Ctrl+Alt",
            "Ctrl+Alt+Q+R",
            "Ctrl+Alt+Nope",
        ] {
            assert!(parse_hotkey(value).is_err(), "value={value}");
        }
    }

    #[test]
    fn invalid_hotkey_does_not_start_a_thread() {
        assert!(spawn("Ctrl+Alt+Nope").is_err());
    }
}
