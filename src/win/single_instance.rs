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
        // SAFETY: the mutex name is a live null-terminated HSTRING. A failed call
        // returns an error instead of a handle.
        let handle =
            unsafe { CreateMutexW(None, true, &HSTRING::from("Local\\Dockwind.SingleInstance")) }
                .context("CreateMutexW failed")?;

        // SAFETY: GetLastError reads the thread's last error after CreateMutexW. The
        // mutex exists either way; ERROR_ALREADY_EXISTS means another instance owns it.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: `handle` was returned by CreateMutexW and is released here
            // exactly once on the failure path.
            if let Err(error) = unsafe { CloseHandle(handle) } {
                tracing::warn!(%error, "failed to close the duplicate-instance mutex");
            }
            bail!("another Dockwind instance is already running");
        }

        Ok(Self(handle))
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the guard owns the mutex handle and releases it exactly once.
        if let Err(error) = unsafe { CloseHandle(self.0) } {
            tracing::warn!(%error, "failed to close the single-instance mutex");
        }
    }
}
