use anyhow::{Context as _, Result, bail};
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
            RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
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
    let status = unsafe { RegSetValueExW(key, &name, None, REG_SZ, Some(&data)) };
    let _ = unsafe { RegCloseKey(key) };

    if status.0 != 0 {
        bail!("RegSetValueExW failed with status {}", status.0);
    }
    Ok(())
}

pub fn disable() -> Result<()> {
    let key = open_run_key()?;
    let name = HSTRING::from(VALUE_NAME);
    let status = unsafe { RegDeleteValueW(key, &name) };
    let _ = unsafe { RegCloseKey(key) };

    if status.0 != 0 && status != ERROR_FILE_NOT_FOUND {
        bail!("RegDeleteValueW failed with status {}", status.0);
    }
    Ok(())
}

fn open_run_key() -> Result<HKEY> {
    let mut key = HKEY::default();
    let subkey = HSTRING::from(RUN_KEY);
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
