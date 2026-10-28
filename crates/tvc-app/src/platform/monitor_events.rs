//! Display, monitor-arrival and resume events that should trigger a reconnect (INV §7), debounced.
//!
//! Windows: one hidden top-level window on its own thread (`WM_DISPLAYCHANGE`, `WM_POWERBROADCAST` resume,
//! `WM_DEVICECHANGE` for the monitor interface class), 500 ms. Linux: NETLINK_KOBJECT_UEVENT drm hotplug, falling
//! back to polling `/sys/class/drm/*/status` every 3 s, 1.5 s. macOS: display reconfiguration + system power-on on
//! a CFRunLoop thread, 500 ms.

use super::Debouncer;
use std::time::Duration;

/// Keeps the event source alive; dropping it stops it (macOS: it lives for the process).
pub struct Watcher {
    _imp: imp::Watcher,
}

/// Calls `on_change` on a background thread once a burst of events has settled. Post to the UI thread from it.
pub fn watch(on_change: impl Fn() + Send + 'static) -> Watcher {
    Watcher { _imp: imp::watch(Debouncer::new(Duration::from_millis(imp::DEBOUNCE_MS), on_change)) }
}

/// Whether a kernel uevent datagram (`ACTION@DEVPATH\0KEY=VALUE\0…`) is a display change: `SUBSYSTEM=drm` with
/// `HOTPLUG=1` or `ACTION=change|add|remove` (add/remove cover MST connectors).
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

#[cfg(windows)]
mod imp {
    use super::Debouncer;
    use crate::platform::wide;
    use std::cell::RefCell;
    use windows_sys::core::GUID;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
        RegisterClassW, RegisterDeviceNotificationW, UnregisterDeviceNotification, DBT_DEVICEARRIVAL,
        DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE,
        DEV_BROADCAST_DEVICEINTERFACE_W, DEV_BROADCAST_HDR, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
        WM_CLOSE, WM_DESTROY, WM_DEVICECHANGE, WM_DISPLAYCHANGE, WM_POWERBROADCAST, WNDCLASSW, WS_OVERLAPPED,
    };

    pub const DEBOUNCE_MS: u64 = 500;
    const MONITOR_INTERFACE: GUID = GUID::from_u128(0xE6F07B5F_EE97_4A90_B076_33F57BF4EAA7);

    thread_local! {
        static DEBOUNCE: RefCell<Option<Debouncer>> = const { RefCell::new(None) };
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

    pub fn watch(debounce: Debouncer) -> Watcher {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("tvc-monitor-events".into()).spawn(move || {
            DEBOUNCE.with(|d| *d.borrow_mut() = Some(debounce));
            let hwnd = create_window();
            let _ = tx.send(hwnd as isize);
            if hwnd.is_null() {
                return;
            }
            // SAFETY: a standard message loop for windows owned by this thread; the notification handle is
            // released when the loop ends (the window is destroyed by then).
            unsafe {
                let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                    dbcc_size: size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                    dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
                    dbcc_reserved: 0,
                    dbcc_classguid: MONITOR_INTERFACE,
                    dbcc_name: [0],
                };
                let filter = (&filter as *const DEV_BROADCAST_DEVICEINTERFACE_W).cast();
                let notify = RegisterDeviceNotificationW(hwnd, filter, DEVICE_NOTIFY_WINDOW_HANDLE);
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    DispatchMessageW(&msg);
                }
                if !notify.is_null() {
                    UnregisterDeviceNotification(notify);
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

    fn schedule() {
        DEBOUNCE.with(|d| {
            if let Some(d) = d.borrow().as_ref() {
                d.trigger();
            }
        });
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_DISPLAYCHANGE => schedule(),
            WM_POWERBROADCAST if matches!(wparam as u32, PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND) => schedule(),
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
                    schedule();
                    return 1;
                }
            }
            WM_DESTROY => {
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(0) };
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
    use super::Debouncer;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    pub const DEBOUNCE_MS: u64 = 1500;

    pub struct Watcher(Arc<AtomicBool>);

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    pub fn watch(debounce: Debouncer) -> Watcher {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let _ = thread::Builder::new().name("tvc-monitor-events".into()).spawn(move || match open_uevent_socket() {
            Some(fd) => read_uevents(fd, &debounce, &flag),
            None => poll_sysfs(&debounce, &flag),
        });
        Watcher(stop)
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

    fn read_uevents(fd: i32, debounce: &Debouncer, stop: &AtomicBool) {
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
                debounce.trigger();
            }
        }
        // SAFETY: our fd, closed once.
        unsafe { libc::close(fd) };
    }

    fn poll_sysfs(debounce: &Debouncer, stop: &AtomicBool) {
        let mut last = connector_status();
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(3));
            let now = connector_status();
            if now != last {
                last = now;
                debounce.trigger();
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
    use super::Debouncer;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicU32, Ordering};

    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type IONotificationPortRef = *mut c_void;

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
    }

    pub const DEBOUNCE_MS: u64 = 500;
    const BEGIN_CONFIGURATION_FLAG: u32 = 1;
    const CAN_SYSTEM_SLEEP: u32 = 0xE000_0270;
    const SYSTEM_WILL_SLEEP: u32 = 0xE000_0280;
    const SYSTEM_HAS_POWERED_ON: u32 = 0xE000_0300;

    /// Shared with the C callbacks through `user_info`; leaked, so it outlives every callback.
    struct Context {
        debounce: Debouncer,
        root_port: AtomicU32,
    }

    extern "C" fn on_display(_display: u32, flags: u32, user_info: *mut c_void) {
        // Called once before ("begin") and once after each change; only the "after" calls count.
        if flags & BEGIN_CONFIGURATION_FLAG == 0 {
            // SAFETY: `user_info` is the leaked Context.
            unsafe { &*(user_info as *const Context) }.debounce.trigger();
        }
    }

    extern "C" fn on_power(refcon: *mut c_void, _service: u32, message: u32, argument: *mut c_void) {
        // SAFETY: `refcon` is the leaked Context.
        let ctx = unsafe { &*(refcon as *const Context) };
        match message {
            // Must be acknowledged or sleep is delayed by 30 s; never veto.
            CAN_SYSTEM_SLEEP | SYSTEM_WILL_SLEEP => {
                let port = ctx.root_port.load(Ordering::Relaxed);
                // SAFETY: the root port from IORegisterForSystemPower and the notification id it handed us.
                unsafe { IOAllowPowerChange(port, argument as isize) };
            }
            SYSTEM_HAS_POWERED_ON => ctx.debounce.trigger(),
            _ => {}
        }
    }

    pub struct Watcher;

    pub fn watch(debounce: Debouncer) -> Watcher {
        let ctx: &'static Context = Box::leak(Box::new(Context { debounce, root_port: AtomicU32::new(0) }));
        let _ = std::thread::Builder::new().name("tvc-monitor-events".into()).spawn(move || {
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
    pub fn watch(_: super::Debouncer) -> Watcher {
        Watcher
    }
}

#[cfg(test)]
mod tests {
    use super::is_drm_change;

    #[test]
    fn recognises_drm_hotplug() {
        assert!(is_drm_change(b"change@/devices/pci/drm/card1\0ACTION=change\0SUBSYSTEM=drm\0HOTPLUG=1\0"));
        assert!(is_drm_change(b"add@/x\0ACTION=add\0SUBSYSTEM=drm\0"));
        assert!(is_drm_change(b"x@/x\0ACTION=bind\0SUBSYSTEM=drm\0HOTPLUG=1\0"));
        assert!(!is_drm_change(b"x@/x\0ACTION=bind\0SUBSYSTEM=drm\0"));
        assert!(!is_drm_change(b"change@/x\0ACTION=change\0SUBSYSTEM=usb\0"));
        assert!(!is_drm_change(b"libudev\0garbage"));
    }
}
