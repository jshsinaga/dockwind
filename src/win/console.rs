use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

/// Explicit command-line options can report errors to the caller's existing console.
pub fn attach_parent() {
    // SAFETY: ATTACH_PARENT_PROCESS is the documented pseudo-ID. A parent
    // without a console (for example, the installer) leaves this process detached.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
