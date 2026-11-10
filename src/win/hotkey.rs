use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey,
        UnregisterHotKey,
    },
    WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY},
};

const HOTKEY_ID: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hotkey {
    modifiers: HOT_KEY_MODIFIERS,
    vk: u32,
}

pub fn spawn(value: &str) -> Result<UnboundedReceiver<()>> {
    let hotkey = parse_hotkey(value).map_err(|error| anyhow!("invalid quit_hotkey: {error}"))?;
    let (sender, receiver) = unbounded();
    let (ready_sender, ready_receiver) = mpsc::channel();

    std::thread::Builder::new()
        .name("quit-hotkey".to_string())
        .spawn(move || unsafe {
            if let Err(error) = RegisterHotKey(None, HOTKEY_ID, hotkey.modifiers, hotkey.vk) {
                let _ = ready_sender.send(Err(anyhow!("RegisterHotKey failed: {error}")));
                return;
            }
            let _ = ready_sender.send(Ok(()));

            let mut message = MSG::default();
            loop {
                let result = GetMessageW(&mut message, None, 0, 0);
                if result.0 <= 0 {
                    break;
                }
                if message.message == WM_HOTKEY {
                    let _ = sender.unbounded_send(());
                    break;
                }
            }

            let _ = UnregisterHotKey(None, HOTKEY_ID);
        })
        .map_err(|error| anyhow!("failed to start hotkey thread: {error}"))?;

    match ready_receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(receiver),
        Ok(Err(error)) => Err(error),
        Err(error) => bail!("hotkey thread did not start: {error}"),
    }
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
}
