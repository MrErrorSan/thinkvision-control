//! System events the engine reacts to: display changes (debounced, trigger a reconnect),
//! display off/on, session lock/unlock and suspend/resume (delivered at once, once per transition).
//!
//! Windows: one hidden top-level window on its own thread — `WM_DISPLAYCHANGE`, `WM_DEVICECHANGE` for the monitor
//! interface class, `WM_POWERBROADCAST` (suspend, resume, `GUID_CONSOLE_DISPLAY_STATE`) and `WM_WTSSESSION_CHANGE`;
//! 500 ms. Linux: NETLINK_KOBJECT_UEVENT drm hotplug (falling back to polling `/sys/class/drm/*/status` every 3 s),
//! 1.5 s; logind `PrepareForSleep` and the session's `LockedHint` read from a `gdbus monitor` child process.
//! macOS: display reconfiguration + IOKit system power on a CFRunLoop thread, NSWorkspace screens sleep/wake and the
//! `com.apple.screenIsLocked`/`Unlocked` distributed notifications; 500 ms.

use super::Debouncer;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Something the OS told us about the display, the session or power.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemEvent {
    /// Displays were added, removed or reconfigured, or the PC resumed; reconnect. Debounced.
    DisplayChanged,
    DisplayOff,
    DisplayOn,
    Locked,
    Unlocked,
    Suspending,
    Resumed,
    /// The light/dark theme changed (Windows: the taskbar's, which picks the tray icon variant).
    ThemeChanged,
}

/// Keeps the event sources alive; dropping it stops them (macOS: they live for the process).
pub struct Watcher {
    _imp: imp::Watcher,
}

/// Calls `on_event` on a background thread for every [`SystemEvent`]. `DisplayChanged` arrives once a burst has
/// settled; the others arrive at once, only when the state actually changes. Post to the UI thread from it.
pub fn watch(on_event: impl Fn(SystemEvent) + Send + 'static) -> Watcher {
    let handler: Handler = Arc::new(Mutex::new(Box::new(on_event)));
    let debounced = handler.clone();
    let debounce = Debouncer::new(Duration::from_millis(imp::DEBOUNCE_MS), move || {
        (lock(&debounced))(SystemEvent::DisplayChanged)
    });
    let sink = Sink { handler, debounce, seen: Mutex::new(Transitions::default()) };
    Watcher { _imp: imp::watch(Arc::new(sink)) }
}

type Handler = Arc<Mutex<Box<dyn Fn(SystemEvent) + Send>>>;

/// Where every OS source sends its events: debounces `DisplayChanged`, drops repeats of the others.
struct Sink {
    handler: Handler,
    debounce: Debouncer,
    seen: Mutex<Transitions>,
}

impl Sink {
    fn send(&self, event: SystemEvent) {
        if event == SystemEvent::DisplayChanged {
            self.debounce.trigger();
        } else if lock(&self.seen).accept(event) {
            (lock(&self.handler))(event);
        }
    }
}

/// Lock that survives a panic in another holder (the data is plain state).
fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The last known display/lock/power state, starting from "on, unlocked, awake", so the initial "display on" that
/// Windows sends at registration (and duplicate resume messages) produce nothing.
#[derive(Default)]
struct Transitions {
    display_off: bool,
    locked: bool,
    suspended: bool,
}

impl Transitions {
    /// Records `event`; true when it is a change worth delivering.
    fn accept(&mut self, event: SystemEvent) -> bool {
        use SystemEvent::*;
        let (flag, value) = match event {
            DisplayChanged | ThemeChanged => return true,
            DisplayOff => (&mut self.display_off, true),
            DisplayOn => (&mut self.display_off, false),
            Locked => (&mut self.locked, true),
            Unlocked => (&mut self.locked, false),
            Suspending => (&mut self.suspended, true),
            Resumed => (&mut self.suspended, false),
        };
        std::mem::replace(flag, value) != value
    }
}

/// `GUID_CONSOLE_DISPLAY_STATE` data: 0 off, 1 on, 2 dimmed (no event).
fn console_display_event(state: u32) -> Option<SystemEvent> {
    match state {
        0 => Some(SystemEvent::DisplayOff),
        1 => Some(SystemEvent::DisplayOn),
        _ => None,
    }
}

/// Whether a kernel uevent datagram (`ACTION@DEVPATH\0KEY=VALUE\0…`) is a display change: `SUBSYSTEM=drm` with
/// `HOTPLUG=1` or `ACTION=change|add|remove` (add/remove cover MST connectors).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_drm_change(message: &[u8]) -> bool {
    let (mut subsystem, mut action, mut hotplug) = (None, None, None);
    for part in message.split(|&b| b == 0) {
        let Some(eq) = part.iter().position(|&b| b == b'=').filter(|&i| i > 0) else { continue };
        let (key, value) = (&part[..eq], &part[eq + 1..]);
        match key {
            b"SUBSYSTEM" => subsystem = Some(value),
            b"ACTION" => action = Some(value),
            b"HOTPLUG" => hotplug = Some(value),
            _ => {}
        }
    }
    subsystem == Some(b"drm")
        && (hotplug == Some(b"1") || matches!(action, Some(b"change" | b"add" | b"remove")))
}

/// One line of `gdbus monitor --system --dest org.freedesktop.login1` as an event. `session` is our session's
/// object path; lock changes of other sessions are ignored.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn logind_event(line: &str, session: Option<&str>) -> Option<SystemEvent> {
    let (path, rest) = line.split_once(": ")?;
    let (signal, args) = rest.split_once(' ').unwrap_or((rest, ""));
    match signal {
        "org.freedesktop.login1.Manager.PrepareForSleep" if path == "/org/freedesktop/login1" => {
            Some(if args.starts_with("(true") { SystemEvent::Suspending } else { SystemEvent::Resumed })
        }
        "org.freedesktop.DBus.Properties.PropertiesChanged"
            if Some(path) == session && args.starts_with("('org.freedesktop.login1.Session',") =>
        {
            if args.contains("'LockedHint': <true>") {
                Some(SystemEvent::Locked)
            } else if args.contains("'LockedHint': <false>") {
                Some(SystemEvent::Unlocked)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// logind's object path for session `id` (sd-bus label escaping: everything but `[A-Za-z0-9]`, and a leading digit,
/// becomes `_xx`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn session_path(id: &str) -> String {
    let mut path = String::from("/org/freedesktop/login1/session/");
    if id.is_empty() {
        path.push('_');
    }
    for (i, b) in id.bytes().enumerate() {
        if b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()) {
            path.push(b as char);
        } else {
            path.push_str(&format!("_{b:02x}"));
        }
    }
    path
}

/// The object path in `gdbus call … User Display` output: `(<('2', objectpath '/org/…/session/_32')>,)`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_display_session(output: &str) -> Option<String> {
    let start = output.find("objectpath '")? + "objectpath '".len();
    let len = output[start..].find('\'')?;
    // "/" means the user has no graphical session.
    Some(output[start..start + len].to_string()).filter(|p| p.starts_with('/') && p.len() > 1)
}

#[cfg(windows)]
mod imp {
    use super::{Sink, SystemEvent};
    use crate::platform::wide;
    use std::cell::RefCell;
    use std::sync::Arc;
    use windows_sys::core::GUID;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Power::{
        RegisterPowerSettingNotification, UnregisterPowerSettingNotification, POWERBROADCAST_SETTING,
    };
    use windows_sys::Win32::System::RemoteDesktop::{
        WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
        RegisterClassW, RegisterDeviceNotificationW, UnregisterDeviceNotification, DBT_DEVICEARRIVAL,
        DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE,
        DEV_BROADCAST_DEVICEINTERFACE_W, DEV_BROADCAST_HDR, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
        PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, WM_CLOSE, WM_DESTROY, WM_DEVICECHANGE, WM_DISPLAYCHANGE,
        WM_POWERBROADCAST, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_OVERLAPPED, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
    };

    pub const DEBOUNCE_MS: u64 = 500;
    const MONITOR_INTERFACE: GUID = GUID::from_u128(0xE6F07B5F_EE97_4A90_B076_33F57BF4EAA7);
    const CONSOLE_DISPLAY_STATE: GUID = GUID::from_u128(0x6FE69556_704A_47A0_8F24_C28D936FDA47);

    thread_local! {
        static SINK: RefCell<Option<Arc<Sink>>> = const { RefCell::new(None) };
    }

    /// The window handle as an integer so the watcher is `Send`; 0 if the window couldn't be created.
    pub struct Watcher(isize);

    impl Drop for Watcher {
        fn drop(&mut self) {
            if self.0 != 0 {
                // SAFETY: posting to a window owned by our thread; a stale handle just fails.
                unsafe { PostMessageW(self.0 as HWND, WM_CLOSE, 0, 0) };
            }
        }
    }

    pub fn watch(sink: Arc<Sink>) -> Watcher {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("tvc-monitor-events".into()).spawn(move || {
            SINK.with(|s| *s.borrow_mut() = Some(sink));
            let hwnd = create_window();
            let _ = tx.send(hwnd as isize);
            if hwnd.is_null() {
                return;
            }
            // SAFETY: a standard message loop for windows owned by this thread. The session notification is
            // dropped in WM_DESTROY (it needs the live window); the other two handles are released after the loop.
            unsafe {
                let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                    dbcc_size: size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                    dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
                    dbcc_reserved: 0,
                    dbcc_classguid: MONITOR_INTERFACE,
                    dbcc_name: [0],
                };
                let filter = (&filter as *const DEV_BROADCAST_DEVICEINTERFACE_W).cast();
                let device = RegisterDeviceNotificationW(hwnd, filter, DEVICE_NOTIFY_WINDOW_HANDLE);
                let display = RegisterPowerSettingNotification(hwnd, &CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
                WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    DispatchMessageW(&msg);
                }
                if !device.is_null() {
                    UnregisterDeviceNotification(device);
                }
                if display != 0 {
                    UnregisterPowerSettingNotification(display);
                }
            }
        });
        Watcher(rx.recv().unwrap_or(0))
    }

    /// A hidden top-level window: `WM_DISPLAYCHANGE` and `WM_POWERBROADCAST` go to top-level windows only, so a
    /// message-only window would miss them.
    fn create_window() -> HWND {
        let class = wide("ThinkVisionControl.MonitorEvents");
        // SAFETY: the class name outlives both calls; registering twice fails harmlessly.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        }
    }

    fn send(event: SystemEvent) {
        if let Some(sink) = SINK.with(|s| s.borrow().clone()) {
            sink.send(event);
        }
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_DISPLAYCHANGE => send(SystemEvent::DisplayChanged),
            WM_POWERBROADCAST => match wparam as u32 {
                PBT_APMSUSPEND => send(SystemEvent::Suspending),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                    send(SystemEvent::Resumed);
                    send(SystemEvent::DisplayChanged);
                }
                PBT_POWERSETTINGCHANGE if lparam != 0 => {
                    let setting = lparam as *const POWERBROADCAST_SETTING;
                    // SAFETY: lParam points at a POWERBROADCAST_SETTING whose `Data` holds `DataLength` bytes; read
                    // through the raw pointer, never a reference to the 1-byte array.
                    let state = unsafe {
                        let ours = guid_eq(&(*setting).PowerSetting, &CONSOLE_DISPLAY_STATE);
                        (ours && (*setting).DataLength >= 4)
                            .then(|| std::ptr::addr_of!((*setting).Data).cast::<u32>().read_unaligned())
                    };
                    if let Some(event) = state.and_then(super::console_display_event) {
                        send(event);
                    }
                    return 1;
                }
                _ => {}
            },
            // Theme switches broadcast "ImmersiveColorSet".
            WM_SETTINGCHANGE if lparam != 0 => {
                // SAFETY: for WM_SETTINGCHANGE a non-null lParam points at a NUL-terminated UTF-16 string.
                let area = unsafe {
                    let p = lparam as *const u16;
                    let len = (0..64).take_while(|&i| *p.add(i) != 0).count();
                    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
                };
                if area == "ImmersiveColorSet" {
                    send(SystemEvent::ThemeChanged);
                }
            }
            WM_WTSSESSION_CHANGE => match wparam as u32 {
                WTS_SESSION_LOCK => send(SystemEvent::Locked),
                WTS_SESSION_UNLOCK => send(SystemEvent::Unlocked),
                _ => {}
            },
            WM_DEVICECHANGE if lparam != 0 && matches!(wparam as u32, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE) => {
                // SAFETY: for these events lParam points at a DEV_BROADCAST_HDR; the class GUID follows it only
                // when the type says device interface.
                let ours = unsafe {
                    let hdr = &*(lparam as *const DEV_BROADCAST_HDR);
                    let iface = &*(lparam as *const DEV_BROADCAST_DEVICEINTERFACE_W);
                    hdr.dbch_devicetype == DBT_DEVTYP_DEVICEINTERFACE
                        && guid_eq(&iface.dbcc_classguid, &MONITOR_INTERFACE)
                };
                if ours {
                    send(SystemEvent::DisplayChanged);
                    return 1;
                }
            }
            WM_DESTROY => {
                // SAFETY: our live window; then end this thread's message loop.
                unsafe {
                    WTSUnRegisterSessionNotification(hwnd);
                    PostQuitMessage(0);
                }
                return 0;
            }
            _ => {}
        }
        // SAFETY: default handling for our own window.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn guid_eq(a: &GUID, b: &GUID) -> bool {
        (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{lock, Sink, SystemEvent};
    use std::io::{BufRead, BufReader};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    pub const DEBOUNCE_MS: u64 = 1500;

    /// Stop flag for the uevent thread and the running `gdbus monitor`, killed on drop.
    pub struct Watcher {
        stop: Arc<AtomicBool>,
        gdbus: Arc<Mutex<Option<Child>>>,
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            kill(&self.gdbus);
        }
    }

    pub fn watch(sink: Arc<Sink>) -> Watcher {
        let stop = Arc::new(AtomicBool::new(false));
        let gdbus = Arc::new(Mutex::new(None));
        let (flag, s) = (stop.clone(), sink.clone());
        let _ = thread::Builder::new().name("tvc-monitor-events".into()).spawn(move || match open_uevent_socket() {
            Some(fd) => read_uevents(fd, &s, &flag),
            None => poll_sysfs(&s, &flag),
        });
        let (flag, slot) = (stop.clone(), gdbus.clone());
        let _ = thread::Builder::new().name("tvc-logind".into()).spawn(move || watch_logind(&sink, &slot, &flag));
        Watcher { stop, gdbus }
    }

    fn kill(slot: &Mutex<Option<Child>>) {
        if let Some(mut child) = lock(slot).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Follows logind through `gdbus monitor` (glib's CLI: no D-Bus crate, no root needed — `dbus-monitor` and
    /// `busctl monitor` need eavesdropping rights on the system bus). Restarts it if it exits; gives up when gdbus
    /// isn't installed, leaving only display changes.
    fn watch_logind(sink: &Sink, slot: &Mutex<Option<Child>>, stop: &AtomicBool) {
        let session = own_session_path();
        while !stop.load(Ordering::Relaxed) {
            let mut command = Command::new("gdbus");
            command
                .args(["monitor", "--system", "--dest", "org.freedesktop.login1"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            // SAFETY: prctl is async-signal-safe; it only makes the child die with this (spawning) thread, so a
            // crash or exit never leaves gdbus behind.
            unsafe {
                command.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                    Ok(())
                });
            }
            let Ok(mut child) = command.spawn() else { return };
            let Some(stdout) = child.stdout.take() else { return };
            *lock(slot) = Some(child);
            if stop.load(Ordering::Relaxed) {
                kill(slot); // dropped while spawning
                return;
            }
            for line in BufReader::new(stdout).split(b'\n') {
                let Ok(line) = line else { break };
                if let Some(event) = super::logind_event(&String::from_utf8_lossy(&line), session.as_deref()) {
                    sink.send(event);
                }
            }
            kill(slot);
            thread::sleep(Duration::from_secs(10));
        }
    }

    /// Our session's object path: `$XDG_SESSION_ID`, else the user's graphical session (`User.Display`), which also
    /// covers apps started by the systemd user manager.
    fn own_session_path() -> Option<String> {
        if let Some(id) = std::env::var("XDG_SESSION_ID").ok().filter(|id| !id.is_empty()) {
            return Some(super::session_path(&id));
        }
        let output = Command::new("gdbus")
            .args([
                "call",
                "--system",
                "--dest",
                "org.freedesktop.login1",
                "--object-path",
                "/org/freedesktop/login1/user/self",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                "org.freedesktop.login1.User",
                "Display",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        super::parse_display_session(&String::from_utf8_lossy(&output.stdout))
    }

    fn open_uevent_socket() -> Option<i32> {
        // SAFETY: plain socket calls; sockaddr_nl is zero-initialised then filled; the fd is closed on failure.
        unsafe {
            let kind = libc::SOCK_DGRAM | libc::SOCK_CLOEXEC;
            let fd = libc::socket(libc::AF_NETLINK, kind, libc::NETLINK_KOBJECT_UEVENT);
            if fd < 0 {
                return None;
            }
            let mut addr: libc::sockaddr_nl = std::mem::zeroed();
            addr.nl_family = libc::AF_NETLINK as u16;
            addr.nl_groups = 1; // raw kernel events
            let len = size_of::<libc::sockaddr_nl>() as u32;
            if libc::bind(fd, (&addr as *const libc::sockaddr_nl).cast(), len) != 0 {
                libc::close(fd);
                return None;
            }
            Some(fd)
        }
    }

    fn read_uevents(fd: i32, sink: &Sink, stop: &AtomicBool) {
        let mut buf = vec![0u8; 16384];
        while !stop.load(Ordering::Relaxed) {
            let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd; the 500 ms timeout lets a drop stop the loop.
            if unsafe { libc::poll(&mut pfd, 1, 500) } <= 0 {
                continue;
            }
            // SAFETY: `buf` is valid for its full length.
            let n = unsafe { libc::recv(fd, buf.as_mut_ptr().cast(), buf.len(), 0) };
            if n > 0 && super::is_drm_change(&buf[..n as usize]) {
                sink.send(SystemEvent::DisplayChanged);
            }
        }
        // SAFETY: our fd, closed once.
        unsafe { libc::close(fd) };
    }

    fn poll_sysfs(sink: &Sink, stop: &AtomicBool) {
        let mut last = connector_status();
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(3));
            let now = connector_status();
            if now != last {
                last = now;
                sink.send(SystemEvent::DisplayChanged);
            }
        }
    }

    /// A fingerprint of every DRM connector's `status`.
    fn connector_status() -> String {
        let Ok(dirs) = std::fs::read_dir("/sys/class/drm") else { return String::new() };
        let mut entries: Vec<_> = dirs.flatten().map(|d| d.path()).collect();
        entries.sort();
        entries
            .iter()
            .filter_map(|dir| {
                let status = std::fs::read_to_string(dir.join("status")).ok()?;
                Some(format!("{}={};", dir.file_name()?.to_string_lossy(), status.trim()))
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{lock, Sink, SystemEvent};
    use std::ffi::{c_char, c_void, CStr};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type IONotificationPortRef = *mut c_void;
    type Id = *mut c_void;
    type Sel = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGDisplayRegisterReconfigurationCallback(
            callback: extern "C" fn(u32, u32, *mut c_void),
            user_info: *mut c_void,
        ) -> i32;
    }
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IORegisterForSystemPower(
            refcon: *mut c_void,
            notify_port: *mut IONotificationPortRef,
            callback: extern "C" fn(*mut c_void, u32, u32, *mut c_void),
            notifier: *mut u32,
        ) -> u32;
        fn IONotificationPortGetRunLoopSource(port: IONotificationPortRef) -> *mut c_void;
        fn IOAllowPowerChange(kernel_port: u32, notification_id: isize) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopCommonModes: CFStringRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: *mut c_void, mode: CFStringRef);
        fn CFRunLoopRun();
        fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, encoding: u32) -> CFStringRef;
    }
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        static NSWorkspaceScreensDidSleepNotification: Id;
        static NSWorkspaceScreensDidWakeNotification: Id;
    }
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
        fn objc_registerClassPair(class: Id);
        fn class_addMethod(class: Id, name: Sel, imp: *const c_void, types: *const c_char) -> u8;
        fn objc_msgSend();
        fn objc_autoreleasePoolPush() -> *mut c_void;
        fn objc_autoreleasePoolPop(pool: *mut c_void);
    }

    pub const DEBOUNCE_MS: u64 = 500;
    const BEGIN_CONFIGURATION_FLAG: u32 = 1;
    const CAN_SYSTEM_SLEEP: u32 = 0xE000_0270;
    const SYSTEM_WILL_SLEEP: u32 = 0xE000_0280;
    const SYSTEM_HAS_POWERED_ON: u32 = 0xE000_0300;
    const UTF8: u32 = 0x0800_0100;

    /// Shared with the C callbacks through `user_info`; leaked, so it outlives every callback.
    struct Context {
        sink: Arc<Sink>,
        root_port: AtomicU32,
    }

    /// For the Objective-C observer, whose methods get no user data.
    static OBSERVER_SINK: Mutex<Option<Arc<Sink>>> = Mutex::new(None);

    extern "C" fn on_display(_display: u32, flags: u32, user_info: *mut c_void) {
        // Called once before ("begin") and once after each change; only the "after" calls count.
        if flags & BEGIN_CONFIGURATION_FLAG == 0 {
            // SAFETY: `user_info` is the leaked Context.
            unsafe { &*(user_info as *const Context) }.sink.send(SystemEvent::DisplayChanged);
        }
    }

    extern "C" fn on_power(refcon: *mut c_void, _service: u32, message: u32, argument: *mut c_void) {
        // SAFETY: `refcon` is the leaked Context.
        let ctx = unsafe { &*(refcon as *const Context) };
        match message {
            // Must be acknowledged or sleep is delayed by 30 s; never veto.
            CAN_SYSTEM_SLEEP | SYSTEM_WILL_SLEEP => {
                if message == SYSTEM_WILL_SLEEP {
                    ctx.sink.send(SystemEvent::Suspending);
                }
                let port = ctx.root_port.load(Ordering::Relaxed);
                // SAFETY: the root port from IORegisterForSystemPower and the notification id it handed us.
                unsafe { IOAllowPowerChange(port, argument as isize) };
            }
            SYSTEM_HAS_POWERED_ON => {
                ctx.sink.send(SystemEvent::Resumed);
                ctx.sink.send(SystemEvent::DisplayChanged);
            }
            _ => {}
        }
    }

    fn observed(event: SystemEvent) {
        if let Some(sink) = lock(&OBSERVER_SINK).clone() {
            sink.send(event);
        }
    }
    extern "C" fn screens_did_sleep(_: Id, _: Sel, _: Id) {
        observed(SystemEvent::DisplayOff);
    }
    extern "C" fn screens_did_wake(_: Id, _: Sel, _: Id) {
        observed(SystemEvent::DisplayOn);
    }
    extern "C" fn screen_locked(_: Id, _: Sel, _: Id) {
        observed(SystemEvent::Locked);
    }
    extern "C" fn screen_unlocked(_: Id, _: Sel, _: Id) {
        observed(SystemEvent::Unlocked);
    }

    fn sel(name: &CStr) -> Sel {
        // SAFETY: a NUL-terminated selector name; the runtime interns it.
        unsafe { sel_registerName(name.as_ptr()) }
    }

    // SAFETY (both): objc_msgSend called through the exact C signature of the selector being sent.
    unsafe fn send_id(obj: Id, s: Sel) -> Id {
        let f: unsafe extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
        f(obj, s)
    }
    unsafe fn add_observer(center: Id, observer: Id, action: Sel, name: Id) {
        let f: unsafe extern "C" fn(Id, Sel, Id, Sel, Id, Id) =
            std::mem::transmute(objc_msgSend as *const ());
        f(center, sel(c"addObserver:selector:name:object:"), observer, action, name, std::ptr::null_mut());
    }

    /// Screens sleep/wake (NSWorkspace) and screen lock/unlock (distributed notifications), via a tiny runtime
    /// class whose four methods forward to `OBSERVER_SINK`. Registered once per process.
    fn observe_screens() {
        type Imp = extern "C" fn(Id, Sel, Id);
        let methods: [(&CStr, Imp); 4] = [
            (c"tvcScreensDidSleep:", screens_did_sleep),
            (c"tvcScreensDidWake:", screens_did_wake),
            (c"tvcScreenLocked:", screen_locked),
            (c"tvcScreenUnlocked:", screen_unlocked),
        ];
        // SAFETY: classes and selectors looked up by name and nil-checked; the observer and the CFString names are
        // kept for the life of the process; a pool covers anything autoreleased.
        unsafe {
            let pool = objc_autoreleasePoolPush();
            let class = objc_allocateClassPair(objc_getClass(c"NSObject".as_ptr()), c"TVCScreenObserver".as_ptr(), 0);
            if !class.is_null() {
                for (name, imp) in methods {
                    class_addMethod(class, sel(name), imp as *const c_void, c"v@:@".as_ptr());
                }
                objc_registerClassPair(class);
                let observer = send_id(class, sel(c"new"));
                let workspace = send_id(objc_getClass(c"NSWorkspace".as_ptr()), sel(c"sharedWorkspace"));
                if !observer.is_null() && !workspace.is_null() {
                    let center = send_id(workspace, sel(c"notificationCenter"));
                    add_observer(center, observer, sel(methods[0].0), NSWorkspaceScreensDidSleepNotification);
                    add_observer(center, observer, sel(methods[1].0), NSWorkspaceScreensDidWakeNotification);
                    let distributed =
                        send_id(objc_getClass(c"NSDistributedNotificationCenter".as_ptr()), sel(c"defaultCenter"));
                    let name = |s: &CStr| CFStringCreateWithCString(std::ptr::null(), s.as_ptr(), UTF8) as Id;
                    add_observer(distributed, observer, sel(methods[2].0), name(c"com.apple.screenIsLocked"));
                    add_observer(distributed, observer, sel(methods[3].0), name(c"com.apple.screenIsUnlocked"));
                }
            }
            objc_autoreleasePoolPop(pool);
        }
    }

    pub struct Watcher;

    pub fn watch(sink: Arc<Sink>) -> Watcher {
        *lock(&OBSERVER_SINK) = Some(sink.clone());
        let ctx: &'static Context = Box::leak(Box::new(Context { sink, root_port: AtomicU32::new(0) }));
        let _ = std::thread::Builder::new().name("tvc-monitor-events".into()).spawn(move || {
            observe_screens();
            let user_info = ctx as *const Context as *mut c_void;
            // SAFETY: callbacks get the leaked context; the power source is added to this thread's run loop,
            // which then runs for the life of the process.
            unsafe {
                CGDisplayRegisterReconfigurationCallback(on_display, user_info);
                let mut port: IONotificationPortRef = std::ptr::null_mut();
                let mut notifier = 0u32;
                let root = IORegisterForSystemPower(user_info, &mut port, on_power, &mut notifier);
                if root == 0 {
                    return;
                }
                ctx.root_port.store(root, Ordering::Relaxed);
                let source = IONotificationPortGetRunLoopSource(port);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                CFRunLoopRun();
            }
        });
        Watcher
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub const DEBOUNCE_MS: u64 = 500;
    pub struct Watcher;
    pub fn watch(_: std::sync::Arc<super::Sink>) -> Watcher {
        Watcher
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SystemEvent::*;

    #[test]
    fn recognises_drm_hotplug() {
        assert!(is_drm_change(b"change@/devices/pci/drm/card1\0ACTION=change\0SUBSYSTEM=drm\0HOTPLUG=1\0"));
        assert!(is_drm_change(b"add@/x\0ACTION=add\0SUBSYSTEM=drm\0"));
        assert!(is_drm_change(b"x@/x\0ACTION=bind\0SUBSYSTEM=drm\0HOTPLUG=1\0"));
        assert!(!is_drm_change(b"x@/x\0ACTION=bind\0SUBSYSTEM=drm\0"));
        assert!(!is_drm_change(b"change@/x\0ACTION=change\0SUBSYSTEM=usb\0"));
        assert!(!is_drm_change(b"libudev\0garbage"));
    }

    #[test]
    fn transitions_drop_repeats_and_the_initial_on() {
        let mut t = Transitions::default();
        assert!(!t.accept(DisplayOn));
        assert!(t.accept(DisplayOff));
        assert!(!t.accept(DisplayOff));
        assert!(t.accept(DisplayOn));
        assert!(t.accept(Locked));
        assert!(t.accept(Unlocked));
        assert!(!t.accept(Resumed));
        assert!(t.accept(Suspending));
        assert!(t.accept(Resumed));
        assert!(!t.accept(Resumed));
        assert!(t.accept(DisplayChanged));
        assert!(t.accept(DisplayChanged));
    }

    #[test]
    fn console_display_states() {
        assert_eq!(console_display_event(0), Some(DisplayOff));
        assert_eq!(console_display_event(1), Some(DisplayOn));
        assert_eq!(console_display_event(2), None);
    }

    #[test]
    fn parses_logind_lines() {
        let me = Some("/org/freedesktop/login1/session/_32");
        let sleep = "/org/freedesktop/login1: org.freedesktop.login1.Manager.PrepareForSleep (true,)";
        let wake = "/org/freedesktop/login1: org.freedesktop.login1.Manager.PrepareForSleep (false,)";
        assert_eq!(logind_event(sleep, me), Some(Suspending));
        assert_eq!(logind_event(wake, None), Some(Resumed));
        let locked = "/org/freedesktop/login1/session/_32: org.freedesktop.DBus.Properties.PropertiesChanged \
                      ('org.freedesktop.login1.Session', {'LockedHint': <true>}, @as [])";
        let unlocked = locked.replace("<true>", "<false>");
        assert_eq!(logind_event(locked, me), Some(Locked));
        assert_eq!(logind_event(&unlocked, me), Some(Unlocked));
        assert_eq!(logind_event(locked, Some("/org/freedesktop/login1/session/c2")), None);
        assert_eq!(logind_event(locked, None), None);
        let idle = "/org/freedesktop/login1/session/_32: org.freedesktop.DBus.Properties.PropertiesChanged \
                    ('org.freedesktop.login1.Session', {'IdleHint': <true>}, @as [])";
        assert_eq!(logind_event(idle, me), None);
        assert_eq!(logind_event("Monitoring signals from all objects owned by org.freedesktop.login1", me), None);
    }

    #[test]
    fn session_paths_follow_sd_bus_escaping() {
        assert_eq!(session_path("2"), "/org/freedesktop/login1/session/_32");
        assert_eq!(session_path("c12"), "/org/freedesktop/login1/session/c12");
        assert_eq!(session_path("a-b"), "/org/freedesktop/login1/session/a_2db");
        assert_eq!(session_path(""), "/org/freedesktop/login1/session/_");
    }

    #[test]
    fn parses_user_display() {
        let out = "(<('2', objectpath '/org/freedesktop/login1/session/_32')>,)\n";
        assert_eq!(parse_display_session(out).as_deref(), Some("/org/freedesktop/login1/session/_32"));
        assert_eq!(parse_display_session("(<('', objectpath '/')>,)"), None);
        assert_eq!(parse_display_session("Error: ..."), None);
    }
}
