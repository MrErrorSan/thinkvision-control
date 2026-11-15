//! One instance per user (per session on Windows); later launches forward their arguments and wait for the reply.
//!
//! Windows: mutex `ThinkVisionControl.Instance.<user SID>` (the installer checks it: keep the names in step) and the
//! duplex pipe `\\.\pipe\ThinkVisionControl.Commands.{user}.{session}`, current user only; the client checks that
//! the pipe's server runs as the same user. Linux: `flock` on `$XDG_RUNTIME_DIR/thinkvision-control.lock` with the
//! socket `$XDG_RUNTIME_DIR/tvc-cmd`. macOS: `$TMPDIR/thinkvision-control.lock` and `$TMPDIR/CoreFxPipe_tvc-cmd`.
//! Without either variable both live in `/tmp/tvc-<uid>/` (mode 0700). A launch sends one line,
//! `wire::encode_request`, and gets one line back, `wire::encode_reply`.

use std::time::{Duration, Instant};

pub use tvc_core::wire::Reply;

/// How long a connected client may take to send its request.
const READ_WAIT: Duration = Duration::from_secs(2);
/// How long a launch waits for the running copy's reply.
const REPLY_WAIT: Duration = Duration::from_secs(15);
/// How long a launch keeps retrying while the running copy answers busy (a one-shot run takes up to 20 s, plus the
/// OSD).
const BUSY_WAIT: Duration = Duration::from_secs(30);
const MAX_LINE: usize = 64 * 1024;

/// Answers one forwarded launch, from any thread. Dropping it without `reply` answers exit 0 with no output.
pub struct Responder(Option<Box<dyn FnOnce(Reply) + Send>>);

impl Responder {
    fn new(send: impl FnOnce(Reply) + Send + 'static) -> Self {
        Self(Some(Box::new(send)))
    }

    pub fn reply(mut self, reply: Reply) {
        if let Some(send) = self.0.take() {
            send(reply);
        }
    }

    /// Tells the launch to try again: this copy can't take its arguments (see `wire::EXIT_BUSY`).
    pub fn busy(self) {
        self.reply(Reply { exit: tvc_core::wire::EXIT_BUSY, stdout: String::new() });
    }
}

/// A responder on its way to the UI thread: dropped there unused (the event loop has quit, or the controller is
/// gone), it answers busy, so the launch retries instead of getting exit 0 with no output.
pub struct BusyUnlessTaken(Option<Responder>);

impl BusyUnlessTaken {
    pub fn new(responder: Responder) -> Self {
        Self(Some(responder))
    }

    pub fn take(mut self) -> Option<Responder> {
        self.0.take()
    }
}

impl Drop for BusyUnlessTaken {
    fn drop(&mut self) {
        if let Some(responder) = self.0.take() {
            responder.busy();
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if let Some(send) = self.0.take() {
            send(Reply::default());
        }
    }
}

/// The outcome of `start`.
pub enum Start {
    /// This process is the primary; call `Listener::listen` to receive later launches' arguments.
    Primary(Listener),
    /// The arguments went to the running instance. `None` when it took them but sent no reply within 15 s
    /// (exit 8); exit now either way.
    Forwarded(Option<Reply>),
    /// The running copy answered busy for 30 s (exit 8).
    Busy,
}

/// Receives forwarded arguments. The single-instance lock is held until the process exits.
pub struct Listener {
    imp: imp::Listener,
}

impl Listener {
    /// Serves forwarded launches on a new thread for the rest of the process. `on_args` runs on that thread with the
    /// launch's arguments and its responder; post both to the UI thread from it. After an error the listener
    /// retries in 500 ms.
    pub fn listen(self, on_args: impl Fn(Vec<String>, Responder) + Send + 'static) {
        let _ = std::thread::Builder::new()
            .name("tvc-instance".into())
            .spawn(move || self.imp.serve(&on_args));
    }
}

/// Becomes the primary or forwards `args` to it. When another copy holds the lock, this alternates a send
/// (250 ms) with waiting for the lock (250 ms) for up to 5 s: a copy that is still starting its listener gets the
/// arguments, and one that is shutting down is taken over instead of this launch quitting silently. While the
/// running copy answers busy (a one-shot run, or closing), it starts over every 250 ms for up to 30 s.
pub fn start(args: &[String]) -> Start {
    let request = tvc_core::wire::encode_request(args);
    let deadline = Instant::now() + BUSY_WAIT;
    loop {
        match attempt(&request) {
            Start::Forwarded(Some(reply)) if reply.exit == tvc_core::wire::EXIT_BUSY => {
                if Instant::now() >= deadline {
                    return Start::Busy;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            start => return start,
        }
    }
}

fn attempt(request: &str) -> Start {
    let lock = match imp::acquire() {
        imp::Acquire::Owned(listener) => return Start::Primary(Listener { imp: listener }),
        imp::Acquire::Busy(lock) => lock,
    };
    #[cfg(target_os = "linux")]
    imp::pass_activation_token();
    let slice = Duration::from_millis(250);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(reply) = imp::send(request, slice) {
            return Start::Forwarded(reply);
        }
        if let Some(listener) = imp::wait(&lock, slice) {
            return Start::Primary(Listener { imp: listener });
        }
    }
    Start::Forwarded(None)
}

/// The XDG activation token for the next window this process opens, so a Wayland compositor lets it take focus:
/// the one this process was started with, else one a later launch left for it. Each token is handed out once.
#[cfg(target_os = "linux")]
pub fn take_activation_token() -> Option<String> {
    use std::sync::Mutex;
    static OWN: Mutex<Option<Option<String>>> = Mutex::new(None);
    let mut own = OWN.lock().unwrap_or_else(|e| e.into_inner());
    let from_env = || std::env::var("XDG_ACTIVATION_TOKEN").ok().filter(|t| !t.is_empty());
    own.get_or_insert_with(from_env).take().or_else(imp::take_passed_token)
}

/// Reads up to the first `\n` (at most 64 KiB) within `timeout`. `read` fills the buffer within the time left and
/// returns the byte count (0 at the end), or None on an error or timeout.
fn read_line(timeout: Duration, mut read: impl FnMut(&mut [u8], Duration) -> Option<usize>) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    while !data.contains(&b'\n') && data.len() < MAX_LINE {
        let left = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())?;
        match read(&mut buf, left)? {
            0 => break,
            n => data.extend_from_slice(&buf[..n]),
        }
    }
    let end = data.iter().position(|&b| b == b'\n').unwrap_or(data.len());
    Some(String::from_utf8_lossy(&data[..end]).into_owned())
}

fn reply_line(reply: &Reply) -> String {
    format!("{}\n", tvc_core::wire::encode_reply(reply))
}

#[cfg(windows)]
mod imp {
    use crate::platform::wide;
    use std::ptr::{null, null_mut};
    use std::thread;
    use std::time::Duration;
    use tvc_core::wire::{decode_reply, decode_request, Reply};
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED,
        GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_ABANDONED, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, WaitNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
        WaitForSingleObject, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    /// A handle to another copy's mutex. Closed on drop, unless `wait` made this process the owner: then it stays
    /// open for the life of the process, since closing the last handle would free the name for a third copy. (The
    /// busy retry opens one per attempt, every 250 ms.)
    pub struct Lock(HANDLE, std::cell::Cell<bool>);

    impl Drop for Lock {
        fn drop(&mut self) {
            if !self.1.get() {
                // SAFETY: our handle, not used after this.
                unsafe { CloseHandle(self.0) };
            }
        }
    }

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
    // SAFETY: a kernel handle may be used and closed from any thread.
    unsafe impl Send for Handle {}

    pub fn acquire() -> Acquire {
        let name = wide(&format!("ThinkVisionControl.Instance.{}", user_of(None).unwrap_or_default()));
        // SAFETY: NUL-terminated name, default security.
        let (handle, existed) = unsafe {
            let h = CreateMutexW(null(), 1, name.as_ptr());
            (h, GetLastError() == ERROR_ALREADY_EXISTS)
        };
        // A null handle (should not happen) is treated as "first": better two copies than none.
        if handle.is_null() || !existed {
            Acquire::Owned(Listener)
        } else {
            Acquire::Busy(Lock(handle, std::cell::Cell::new(false)))
        }
    }

    /// Waits for the mutex; a released or abandoned (crashed owner) mutex makes this process primary.
    pub fn wait(lock: &Lock, timeout: Duration) -> Option<Listener> {
        // SAFETY: a valid mutex handle that we keep open.
        match unsafe { WaitForSingleObject(lock.0, timeout.as_millis() as u32) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => {
                lock.1.set(true); // ours now: keep the handle open
                Some(Listener)
            }
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

    /// The string SID of the user a process runs as: `pid`, or this process when None.
    fn user_of(pid: Option<u32>) -> Option<String> {
        // SAFETY: handles are closed by `Handle` (the current-process pseudo-handle needs no close); the token
        // buffer is sized by the API; the SID string is LocalAlloc'd by the API and freed here.
        unsafe {
            let opened = match pid {
                Some(pid) => {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if h.is_null() {
                        return None;
                    }
                    Some(Handle(h))
                }
                None => None,
            };
            let process = match &opened {
                Some(h) => h.0,
                None => GetCurrentProcess(),
            };
            let mut token = null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
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
            let mut sid_str = null_mut();
            if ConvertSidToStringSidW(sid, &mut sid_str) == 0 {
                return None;
            }
            let sid_len = (0..).take_while(|&i| *sid_str.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(sid_str, sid_len));
            LocalFree(sid_str.cast());
            Some(text)
        }
    }

    /// Runs one overlapped `call` on `pipe` and waits up to `ms` for it: the bytes moved, or None on an error or
    /// timeout (the call is then cancelled).
    unsafe fn overlapped(pipe: HANDLE, ms: u32, call: impl FnOnce(*mut OVERLAPPED) -> BOOL) -> Option<u32> {
        let event = CreateEventW(null(), 1, 0, null());
        if event.is_null() {
            return None;
        }
        let event = Handle(event);
        let mut ov = OVERLAPPED { hEvent: event.0, ..Default::default() };
        let ptr: *mut OVERLAPPED = &mut ov;
        if call(ptr) == 0 {
            match GetLastError() {
                // A client connected between CreateNamedPipeW and ConnectNamedPipe.
                ERROR_PIPE_CONNECTED => return Some(0),
                ERROR_IO_PENDING => {
                    if WaitForSingleObject(event.0, ms) != WAIT_OBJECT_0 {
                        CancelIoEx(pipe, &ov);
                    }
                }
                _ => return None,
            }
        }
        let mut moved = 0u32;
        // Waits for the cancelled call too, so `ov` outlives it.
        (GetOverlappedResult(pipe, &ov, &mut moved, 1) != 0).then_some(moved)
    }

    fn write(pipe: &Handle, bytes: &[u8]) -> bool {
        let ms = super::READ_WAIT.as_millis() as u32;
        // SAFETY: `bytes` outlives the call, which `overlapped` waits for.
        let written = unsafe {
            overlapped(pipe.0, ms, |ov| WriteFile(pipe.0, bytes.as_ptr(), bytes.len() as u32, null_mut(), ov))
        };
        written == Some(bytes.len() as u32)
    }

    fn read(pipe: &Handle, timeout: Duration) -> Option<String> {
        super::read_line(timeout, |buf, left| {
            let ms = left.as_millis() as u32;
            // SAFETY: `buf` outlives the call, which `overlapped` waits for.
            let got = unsafe {
                overlapped(pipe.0, ms, |ov| ReadFile(pipe.0, buf.as_mut_ptr(), buf.len() as u32, null_mut(), ov))
            };
            got.map(|n| n as usize)
        })
    }

    pub fn send(request: &str, timeout: Duration) -> Option<Option<Reply>> {
        let name = pipe_name();
        // SAFETY: NUL-terminated name; the handle is closed by `Handle`.
        let pipe = unsafe {
            if WaitNamedPipeW(name.as_ptr(), timeout.as_millis() as u32) == 0 {
                return None;
            }
            let access = GENERIC_READ | GENERIC_WRITE;
            let h = CreateFileW(name.as_ptr(), access, 0, null(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED, null_mut());
            if h == INVALID_HANDLE_VALUE {
                return None;
            }
            Handle(h)
        };
        // Only a server running as this user gets the arguments (another account could have squatted the name).
        let mut pid = 0u32;
        // SAFETY: plain out-parameter on our pipe handle.
        if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut pid) } == 0 {
            return None;
        }
        if !user_of(None).is_some_and(|me| user_of(Some(pid)) == Some(me)) {
            return None;
        }
        // This launch came from the user, so it may take the foreground; pass that on, or Windows opens the
        // running copy's window behind the active one.
        // SAFETY: no pointers.
        unsafe { AllowSetForegroundWindow(pid) };
        if !write(&pipe, format!("{request}\n").as_bytes()) {
            return None;
        }
        Some(read(&pipe, super::REPLY_WAIT).and_then(|line| decode_reply(&line)))
    }

    /// `D:P(A;;GA;;;<current user SID>)`: only this user may open the pipe.
    fn current_user_only() -> Option<SECURITY_ATTRIBUTES> {
        let sddl = wide(&format!("D:P(A;;GA;;;{})", user_of(None)?));
        // SAFETY: the descriptor is LocalAlloc'd by the API and lives for the process (reused by every instance).
        unsafe {
            let mut sd = null_mut();
            let (rev, null) = (SDDL_REVISION_1, null_mut());
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

    /// A new pipe instance. `first`: fail if the name already exists, so nobody else's pipe is joined.
    fn create(name: &[u16], sa: Option<&SECURITY_ATTRIBUTES>, first: bool) -> Option<Handle> {
        let mut mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: NUL-terminated name; `sa` (if any) outlives the call.
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                mode,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                super::MAX_LINE as u32,
                4096,
                0,
                sa.map_or(null(), |s| s as *const _),
            )
        };
        (h != INVALID_HANDLE_VALUE).then_some(Handle(h))
    }

    impl Listener {
        pub fn serve(self, on_args: &dyn Fn(Vec<String>, super::Responder)) {
            let name = pipe_name();
            let sa = current_user_only();
            let mut next = None;
            loop {
                // With no instance of ours open, the name must be new.
                let Some(pipe) = next.take().or_else(|| create(&name, sa.as_ref(), true)) else {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                };
                // SAFETY: our pipe handle; `overlapped` waits for the call.
                if unsafe { overlapped(pipe.0, INFINITE, |ov| ConnectNamedPipe(pipe.0, ov)) }.is_none() {
                    continue;
                }
                // Open the next instance while this one is still ours, so the name is never free.
                next = create(&name, sa.as_ref(), false);
                let Some(args) = read(&pipe, super::READ_WAIT).and_then(|line| decode_request(&line)) else {
                    continue;
                };
                let responder = super::Responder::new(move |reply| {
                    let _ = thread::Builder::new().name("tvc-reply".into()).spawn(move || {
                        // Then wait for the client to hang up: closing first could drop the unread reply.
                        if write(&pipe, super::reply_line(&reply).as_bytes()) {
                            let _ = read(&pipe, super::READ_WAIT);
                        }
                    });
                });
                on_args(args, responder);
            }
        }
    }
}

#[cfg(unix)]
mod imp {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;
    use tvc_core::wire::{decode_reply, decode_request, Reply};

    /// The lock file, open for the life of the process so the `flock` holds, and the socket path.
    pub struct Lock(File, PathBuf);

    /// `socket` None: the lock file or folder belongs to someone else, so run as primary without forwarding.
    pub struct Listener {
        socket: Option<PathBuf>,
    }

    pub enum Acquire {
        Owned(Listener),
        Busy(Lock),
    }

    /// The lock file and socket paths; None when the `/tmp` fallback folder isn't ours.
    fn paths() -> Option<(PathBuf, PathBuf)> {
        #[cfg(target_os = "macos")]
        let (var, socket) = ("TMPDIR", "CoreFxPipe_tvc-cmd");
        #[cfg(not(target_os = "macos"))]
        let (var, socket) = ("XDG_RUNTIME_DIR", "tvc-cmd");
        let dir = std::env::var_os(var).map(PathBuf::from).filter(|d| !d.as_os_str().is_empty() && d.is_dir());
        let dir = match dir {
            Some(dir) => dir,
            None => private_tmp()?,
        };
        Some((dir.join("thinkvision-control.lock"), dir.join(socket)))
    }

    /// `/tmp/tvc-<uid>`, created at mode 0700; None when it exists but isn't a folder of ours.
    fn private_tmp() -> Option<PathBuf> {
        // SAFETY: no preconditions.
        let uid = unsafe { libc::getuid() };
        let dir = PathBuf::from(format!("/tmp/tvc-{uid}"));
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
        let meta = std::fs::symlink_metadata(&dir).ok()?;
        if !meta.is_dir() || meta.uid() != uid {
            return None;
        }
        if meta.mode() & 0o777 != 0o700 {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
        }
        Some(dir)
    }

    fn try_lock(file: &File) -> bool {
        // SAFETY: a valid open fd; the kernel drops the lock when the process dies.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
    }

    /// Holds the lock: keep the file open for the process lifetime and clear a socket left by a crashed primary.
    fn owned(file: File, socket: PathBuf) -> Listener {
        std::mem::forget(file);
        let _ = std::fs::remove_file(&socket);
        Listener { socket: Some(socket) }
    }

    pub fn acquire() -> Acquire {
        let Some((lock, socket)) = paths() else { return Acquire::Owned(Listener { socket: None }) };
        let opened = OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(lock);
        let file = match opened {
            Ok(f) => f,
            // Lock file owned by someone else: don't block startup, run without forwarding.
            Err(_) => return Acquire::Owned(Listener { socket: None }),
        };
        if try_lock(&file) {
            Acquire::Owned(owned(file, socket))
        } else {
            Acquire::Busy(Lock(file, socket))
        }
    }

    pub fn wait(lock: &Lock, timeout: Duration) -> Option<Listener> {
        if try_lock(&lock.0) {
            let file = lock.0.try_clone().ok()?;
            return Some(owned(file, lock.1.clone()));
        }
        thread::sleep(timeout);
        None
    }

    fn read(stream: &mut UnixStream, timeout: Duration) -> Option<String> {
        super::read_line(timeout, |buf, left| {
            stream.set_read_timeout(Some(left)).ok()?;
            stream.read(buf).ok()
        })
    }

    /// The file a later launch leaves its activation token in for the running copy. The wire request carries only
    /// the arguments, which older copies parse strictly, so the token travels beside it.
    #[cfg(target_os = "linux")]
    fn token_path() -> Option<PathBuf> {
        Some(paths()?.0.with_file_name("thinkvision-control.activation"))
    }

    /// Leaves this launch's `XDG_ACTIVATION_TOKEN` (set by the desktop for an app started from a launcher) for the
    /// running copy, which opens its window with it.
    #[cfg(target_os = "linux")]
    pub fn pass_activation_token() {
        let Some(token) = std::env::var("XDG_ACTIVATION_TOKEN").ok().filter(|t| !t.is_empty()) else { return };
        let Some(path) = token_path() else { return };
        let file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path);
        if let Ok(mut file) = file {
            let _ = file.write_all(token.as_bytes());
        }
    }

    /// The token a later launch left, removed as it is read.
    #[cfg(target_os = "linux")]
    pub fn take_passed_token() -> Option<String> {
        let path = token_path()?;
        let token = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        Some(token.trim().to_owned()).filter(|t| !t.is_empty())
    }

    pub fn send(request: &str, _timeout: Duration) -> Option<Option<Reply>> {
        let mut stream = UnixStream::connect(paths()?.1).ok()?;
        stream.set_write_timeout(Some(super::READ_WAIT)).ok()?;
        stream.write_all(format!("{request}\n").as_bytes()).ok()?;
        Some(read(&mut stream, super::REPLY_WAIT).and_then(|line| decode_reply(&line)))
    }

    impl Listener {
        pub fn serve(self, on_args: &dyn Fn(Vec<String>, super::Responder)) {
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
                    let Ok(mut stream) = stream else {
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    };
                    let Some(args) = read(&mut stream, super::READ_WAIT).and_then(|line| decode_request(&line)) else {
                        continue;
                    };
                    let _ = stream.set_write_timeout(Some(super::READ_WAIT));
                    let responder = super::Responder::new(move |reply| {
                        let _ = stream.write_all(super::reply_line(&reply).as_bytes());
                    });
                    on_args(args, responder);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    /// Feeds `chunks` to `read_line` one read at a time, then the end of the stream.
    fn read_chunks(chunks: &[&str]) -> Option<String> {
        let mut chunks = chunks.iter();
        super::read_line(Duration::from_secs(1), |buf, _| {
            let chunk = chunks.next().map_or(&b""[..], |c| c.as_bytes());
            buf[..chunk.len()].copy_from_slice(chunk);
            Some(chunk.len())
        })
    }

    #[test]
    fn a_responder_dropped_on_the_way_answers_busy() {
        let (tx, rx) = std::sync::mpsc::channel();
        let again = tx.clone();
        drop(super::BusyUnlessTaken::new(super::Responder::new(move |reply| tx.send(reply).unwrap())));
        assert_eq!(rx.recv().unwrap().exit, tvc_core::wire::EXIT_BUSY);
        let taken = super::BusyUnlessTaken::new(super::Responder::new(move |reply| again.send(reply).unwrap())).take();
        taken.unwrap().reply(super::Reply { exit: 0, stdout: "ok\n".into() });
        assert_eq!(rx.recv().unwrap(), super::Reply { exit: 0, stdout: "ok\n".into() });
        assert!(rx.recv().is_err()); // the taken one answered once, not busy as well
    }

    #[test]
    fn reads_one_line() {
        assert_eq!(read_chunks(&["[\"--pre", "set\",\"day\"]\nrest"]).as_deref(), Some(r#"["--preset","day"]"#));
        assert_eq!(read_chunks(&["no newline"]).as_deref(), Some("no newline"));
        assert_eq!(read_chunks(&[]).as_deref(), Some(""));
    }
}
