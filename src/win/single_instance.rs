use anyhow::{Context as _, Result, bail};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::CreateMutexW,
    },
    core::HSTRING,
};

pub struct SingleInstanceGuard(HANDLE);

impl SingleInstanceGuard {
    pub fn acquire() -> Result<Self> {
        let handle =
            unsafe { CreateMutexW(None, true, &HSTRING::from("Local\\Dockwind.SingleInstance")) }
                .context("CreateMutexW failed")?;

        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let _ = unsafe { CloseHandle(handle) };
            bail!("another Dockwind instance is already running");
        }

        Ok(Self(handle))
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
