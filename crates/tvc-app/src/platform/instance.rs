//! One instance per user (per session on Windows); later launches forward their arguments.
//!
//! Windows: mutex `ThinkVisionControl.SingleInstance` (the installer's AppMutex) and the pipe
//! `\\.\pipe\ThinkVisionControl.Commands.{user}.{session}`, current user only. Linux: `flock` on
//! `$XDG_RUNTIME_DIR/thinkvision-control.lock` with the socket `$XDG_RUNTIME_DIR/tvc-cmd` (fallback
//! `/tmp/thinkvision-control-{user}.lock`, `/tmp/CoreFxPipe_tvc-cmd`). macOS: `$TMPDIR/thinkvision-control.lock`
//! and `$TMPDIR/CoreFxPipe_tvc-cmd`. Arguments travel joined with `\n`.

use std::time::{Duration, Instant};

/// The outcome of `start`.
pub enum Start {
    /// This process is the primary; call `Listener::listen` to receive later launches' arguments.
    Primary(Listener),
    /// The arguments went to the running instance (or it never answered within 5 s); exit now.
    Forwarded,
}

/// Receives forwarded arguments. The single-instance lock is held until the process exits.
pub struct Listener {
    imp: imp::Listener,
}

impl Listener {
    /// Serves forwarded launches on a new thread for the rest of the process. `on_args` runs on that thread with the
    /// non-empty lines; post to the UI thread from it. After an error the listener retries in 500 ms.
    pub fn listen(self, on_args: impl Fn(Vec<String>) + Send + 'static) {
        let _ = std::thread::Builder::new()
            .name("tvc-instance".into())
            .spawn(move || self.imp.serve(&on_args));
    }
}

/// Becomes the primary or forwards `args` to it. When another copy holds the lock, this alternates a send
/// (250 ms) with waiting for the lock (250 ms) for up to 5 s: a copy that is still starting its listener gets the
/// arguments, and one that is shutting down is taken over instead of this launch quitting silently.
pub fn start(args: &[String]) -> Start {
    let payload = args.join("\n");
    let lock = match imp::acquire() {
        imp::Acquire::Owned(listener) => return Start::Primary(Listener { imp: listener }),
        imp::Acquire::Busy(lock) => lock,
    };
    let slice = Duration::from_millis(250);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if imp::send(&payload, slice) {
            return Start::Forwarded;
        }
        if let Some(listener) = imp::wait(&lock, slice) {
            return Start::Primary(Listener { imp: listener });
        }
    }
    Start::Forwarded
}

fn split_args(text: &str) -> Vec<String> {
    text.split('\n').filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

#[cfg(windows)]
mod imp {
    use crate::platform::wide;
    use std::ptr::null;
    use std::thread;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_ABANDONED, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, OPEN_EXISTING, PIPE_ACCESS_INBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, WaitNamedPipeW,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        CreateMutexW, GetCurrentProcess, GetCurrentProcessId, OpenProcessToken, WaitForSingleObject,
    };
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    /// The mutex handle, kept open (and owned) for the life of the process; never closed.
    pub struct Lock(HANDLE);

    pub struct Listener;

    pub enum Acquire {
        Owned(Listener),
        Busy(Lock),
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: we own this handle.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn acquire() -> Acquire {
        let name = wide("ThinkVisionControl.SingleInstance");
        // SAFETY: NUL-terminated name, default security.
        let (handle, existed) = unsafe {
            let h = CreateMutexW(null(), 1, name.as_ptr());
            (h, GetLastError() == ERROR_ALREADY_EXISTS)
        };
        // A null handle (should not happen) is treated as "first": better two copies than none.
        if handle.is_null() || !existed {
            Acquire::Owned(Listener)
        } else {
            Acquire::Busy(Lock(handle))
        }
    }

    /// Waits for the mutex; a released or abandoned (crashed owner) mutex makes this process primary.
    pub fn wait(lock: &Lock, timeout: Duration) -> Option<Listener> {
        // SAFETY: a valid mutex handle that we keep open.
        match unsafe { WaitForSingleObject(lock.0, timeout.as_millis() as u32) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Some(Listener),
            _ => None,
        }
    }

    fn pipe_name() -> Vec<u16> {
        let mut buf = [0u16; 257];
        let mut len = buf.len() as u32;
        // SAFETY: the buffer and its length in characters are passed together.
        let user = if unsafe { GetUserNameW(buf.as_mut_ptr(), &mut len) } != 0 {
            String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize])
        } else {
            std::env::var("USERNAME").unwrap_or_default()
        };
        let mut session = 0u32;
        // SAFETY: plain out-parameter.
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
        wide(&format!(r"\\.\pipe\ThinkVisionControl.Commands.{user}.{session}"))
    }

    pub fn send(payload: &str, timeout: Duration) -> bool {
        let name = pipe_name();
        // SAFETY: NUL-terminated name; the handle is closed by `Handle`; the write buffer outlives the call.
        unsafe {
            if WaitNamedPipeW(name.as_ptr(), timeout.as_millis() as u32) == 0 {
                return false;
            }
            let h = CreateFileW(name.as_ptr(), GENERIC_WRITE, 0, null(), OPEN_EXISTING, 0, std::ptr::null_mut());
            if h == INVALID_HANDLE_VALUE {
                return false;
            }
            let pipe = Handle(h);
            // This launch came from the user, so it may take the foreground; pass that on, or Windows opens the
            // running copy's window behind the active one.
            let mut pid = 0u32;
            if GetNamedPipeServerProcessId(pipe.0, &mut pid) != 0 {
                AllowSetForegroundWindow(pid);
            }
            let bytes = payload.as_bytes();
            let mut written = 0u32;
            WriteFile(pipe.0, bytes.as_ptr(), bytes.len() as u32, &mut written, std::ptr::null_mut()) != 0
                && written as usize == bytes.len()
        }
    }

    /// `D:P(A;;GA;;;<current user SID>)`: only this user may open the pipe.
    fn current_user_only() -> Option<SECURITY_ATTRIBUTES> {
        // SAFETY: token and buffers are sized by the API; the SID string and descriptor are LocalAlloc'd by the
        // API — the string is freed here, the descriptor lives for the process (reused by every pipe instance).
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let token = Handle(token);
            let mut buf = [0u64; 64];
            let mut len = 0u32;
            let size = size_of_val(&buf) as u32;
            if GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), size, &mut len) == 0 {
                return None;
            }
            let sid = (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid;
            let mut sid_str = std::ptr::null_mut();
            if ConvertSidToStringSidW(sid, &mut sid_str) == 0 {
                return None;
            }
            let sid_len = (0..).take_while(|&i| *sid_str.add(i) != 0).count();
            let sid_text = String::from_utf16_lossy(std::slice::from_raw_parts(sid_str, sid_len));
            LocalFree(sid_str.cast());
            let sddl = wide(&format!("D:P(A;;GA;;;{sid_text})"));
            let mut sd = std::ptr::null_mut();
            let (rev, null) = (SDDL_REVISION_1, std::ptr::null_mut());
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), rev, &mut sd, null) == 0 {
                return None;
            }
            Some(SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            })
        }
    }

    impl Listener {
        pub fn serve(self, on_args: &dyn Fn(Vec<String>)) {
            let name = pipe_name();
            let sa = current_user_only();
            loop {
                match receive(&name, sa.as_ref()) {
                    Some(text) => on_args(super::split_args(&text)),
                    None => thread::sleep(Duration::from_millis(500)),
                }
            }
        }
    }

    /// One pipe instance, one client, read to EOF.
    fn receive(name: &[u16], sa: Option<&SECURITY_ATTRIBUTES>) -> Option<String> {
        // SAFETY: NUL-terminated name; `sa` (if any) outlives the call; the handle is closed by `Handle`.
        unsafe {
            let h = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                4096,
                0,
                sa.map_or(null(), |s| s as *const _),
            );
            if h == INVALID_HANDLE_VALUE {
                return None;
            }
            let pipe = Handle(h);
            if ConnectNamedPipe(pipe.0, std::ptr::null_mut()) == 0 && GetLastError() != ERROR_PIPE_CONNECTED {
                return None;
            }
            let mut data = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                let mut read = 0u32;
                let ok = ReadFile(pipe.0, buf.as_mut_ptr(), buf.len() as u32, &mut read, std::ptr::null_mut()) != 0;
                data.extend_from_slice(&buf[..read as usize]);
                // EOF (ERROR_BROKEN_PIPE) once the client closes; that's the normal end.
                if !ok || read == 0 {
                    break;
                }
            }
            DisconnectNamedPipe(pipe.0);
            Some(String::from_utf8_lossy(&data).into_owned())
        }
    }
}

#[cfg(unix)]
mod imp {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    /// The lock file, open for the life of the process so the `flock` holds.
    pub struct Lock(File);

    /// `socket` None: the lock file belongs to someone else, so run as primary without forwarding.
    pub struct Listener {
        socket: Option<PathBuf>,
    }

    pub enum Acquire {
        Owned(Listener),
        Busy(Lock),
    }

    fn paths() -> (PathBuf, PathBuf) {
        #[cfg(target_os = "macos")]
        {
            let tmp = std::env::var_os("TMPDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
            (tmp.join("thinkvision-control.lock"), tmp.join("CoreFxPipe_tvc-cmd"))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
            match runtime.filter(|d| !d.as_os_str().is_empty() && d.is_dir()) {
                Some(run) => (run.join("thinkvision-control.lock"), run.join("tvc-cmd")),
                None => {
                    let user = std::env::var("USER").unwrap_or_default();
                    (
                        PathBuf::from(format!("/tmp/thinkvision-control-{user}.lock")),
                        PathBuf::from("/tmp/CoreFxPipe_tvc-cmd"),
                    )
                }
            }
        }
    }

    fn try_lock(file: &File) -> bool {
        // SAFETY: a valid open fd; the kernel drops the lock when the process dies.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
    }

    /// Holds the lock: keep the file open for the process lifetime and clear a socket left by a crashed primary.
    fn owned(file: File) -> Listener {
        std::mem::forget(file);
        let socket = paths().1;
        let _ = std::fs::remove_file(&socket);
        Listener { socket: Some(socket) }
    }

    pub fn acquire() -> Acquire {
        let opened = OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(paths().0);
        let file = match opened {
            Ok(f) => f,
            // Lock file owned by someone else: don't block startup, run without forwarding.
            Err(_) => return Acquire::Owned(Listener { socket: None }),
        };
        if try_lock(&file) {
            Acquire::Owned(owned(file))
        } else {
            Acquire::Busy(Lock(file))
        }
    }

    pub fn wait(lock: &Lock, timeout: Duration) -> Option<Listener> {
        if try_lock(&lock.0) {
            let file = lock.0.try_clone().ok()?;
            return Some(owned(file));
        }
        thread::sleep(timeout);
        None
    }

    pub fn send(payload: &str, _timeout: Duration) -> bool {
        UnixStream::connect(paths().1).and_then(|mut s| s.write_all(payload.as_bytes())).is_ok()
    }

    impl Listener {
        pub fn serve(self, on_args: &dyn Fn(Vec<String>)) {
            let Some(path) = self.socket else { return };
            loop {
                let listener = match UnixListener::bind(&path) {
                    Ok(l) => l,
                    Err(_) => {
                        let _ = std::fs::remove_file(&path);
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                };
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                for stream in listener.incoming() {
                    let mut text = String::new();
                    match stream.and_then(|mut s| s.read_to_string(&mut text)) {
                        Ok(_) => on_args(super::split_args(&text)),
                        Err(_) => thread::sleep(Duration::from_millis(500)),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn splits_forwarded_lines() {
        assert_eq!(super::split_args("--preset\nday\n\n"), ["--preset", "day"]);
        assert!(super::split_args("").is_empty());
    }
}
