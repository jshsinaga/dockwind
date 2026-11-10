use anyhow::{Context as _, Result, bail};
use tracing::warn;
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
            REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
            RegSetValueExW,
        },
    },
    core::HSTRING,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "Dockwind";

pub fn enable() -> Result<()> {
    let executable =
        std::env::current_exe().context("failed to resolve the Dockwind executable")?;
    let command = format!("\"{}\"", executable.display());
    let data = command
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<u8>>();

    let key = open_run_key()?;
    let name = HSTRING::from(VALUE_NAME);
    // SAFETY: `key` was opened by this function, the name is a live null-terminated
    // HSTRING, and the data buffer is valid for its length. REG_SZ copies the bytes.
    let status = unsafe { RegSetValueExW(key, &name, None, REG_SZ, Some(&data)) };
    close_key(key);

    if status.0 != 0 {
        bail!("RegSetValueExW failed with status {}", status.0);
    }
    Ok(())
}

pub fn disable() -> Result<()> {
    let key = open_run_key()?;
    let name = HSTRING::from(VALUE_NAME);
    // SAFETY: `key` was opened by this function and the name is a valid HSTRING.
    let status = unsafe { RegDeleteValueW(key, &name) };
    close_key(key);

    // A missing value already means autostart is disabled.
    if status.0 != 0 && status != ERROR_FILE_NOT_FOUND {
        bail!("RegDeleteValueW failed with status {}", status.0);
    }
    Ok(())
}

pub fn is_enabled() -> Result<bool> {
    let mut key = HKEY::default();
    let subkey = HSTRING::from(RUN_KEY);
    // SAFETY: `key` is a valid out-parameter and the subkey is a live HSTRING.
    let status =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_QUERY_VALUE, &mut key) };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if status.0 != 0 {
        bail!("RegOpenKeyExW failed with status {}", status.0);
    }

    let name = HSTRING::from(VALUE_NAME);
    // SAFETY: `key` was opened above. Passing null data and size pointers is allowed
    // and only queries the value's presence and type.
    let query = unsafe { RegQueryValueExW(key, &name, None, None, None, None) };
    close_key(key);

    if query == ERROR_FILE_NOT_FOUND {
        Ok(false)
    } else if query.0 != 0 {
        bail!("RegQueryValueExW failed with status {}", query.0);
    } else {
        Ok(true)
    }
}

fn open_run_key() -> Result<HKEY> {
    let mut key = HKEY::default();
    let subkey = HSTRING::from(RUN_KEY);
    // SAFETY: `key` is a valid out-parameter, the subkey is a live HSTRING, and the
    // user-scoped Run key is created with the requested access.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &subkey,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if status.0 != 0 {
        bail!("RegCreateKeyExW failed with status {}", status.0);
    }
    Ok(key)
}

fn close_key(key: HKEY) {
    // SAFETY: every caller passes a key opened or created by this module. It is
    // closed exactly once.
    let status = unsafe { RegCloseKey(key) };
    if status.0 != 0 {
        warn!(status = status.0, "failed to close a registry key");
    }
}
