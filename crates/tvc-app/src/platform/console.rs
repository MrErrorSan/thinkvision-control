//! Command-line output from the GUI-subsystem Windows exe.

/// Windows: attaches to the calling terminal's console (`AttachConsole(ATTACH_PARENT_PROCESS)`) so `println!`
/// reaches it. Std handles the parent redirected (`> file`, pipes) are kept; missing ones are pointed at the
/// console. Rust's std looks the handle up on every write, so nothing needs re-initialising. No-op elsewhere and
/// when there is no parent console (started from Explorer).
pub fn attach() {
    imp::attach();
}

#[cfg(windows)]
mod imp {
    use crate::platform::wide;
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    fn missing(handle: HANDLE) -> bool {
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    }

    pub fn attach() {
        // SAFETY: plain Win32 calls; the CONOUT$ handle is handed to the process's std handle table for good.
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return;
            }
            let ids: [STD_HANDLE; 2] = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
            if !ids.iter().any(|&id| missing(GetStdHandle(id))) {
                return;
            }
            let name = wide("CONOUT$");
            let console = CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if missing(console) {
                return;
            }
            for id in ids {
                if missing(GetStdHandle(id)) {
                    SetStdHandle(id, console);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn attach() {}
}
