//! Taskbar theme, which picks the tray icon variant (`Tray.OnLight` / `Tray.OnDark`).

/// True when the taskbar is light. Windows: `SystemUsesLightTheme` (the taskbar follows "Windows mode", not the app
/// mode). Linux: the desktop portal's `color-scheme` prefers light, outside GNOME (whose top bar is dark in both
/// modes). False elsewhere, and while the portal hasn't answered.
pub fn is_light() -> bool {
    #[cfg(windows)]
    {
        use super::wide;
        use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
        let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let name = wide("SystemUsesLightTheme");
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: valid NUL-terminated strings and a DWORD-sized out buffer whose size is passed.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut size,
            )
        };
        status == 0 && value != 0
    }
    #[cfg(target_os = "linux")]
    {
        !super::is_gnome(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref()) && portal::SCHEME.load(std::sync::atomic::Ordering::Relaxed) == portal::PREFER_LIGHT
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    false
}

/// Linux: reads the portal's `color-scheme` now and on every change, on a worker thread, and calls `on_change`
/// after each. Starts once per process; later calls do nothing.
#[cfg(target_os = "linux")]
pub fn watch(on_change: impl Fn() + Send + 'static) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("tvc-color-scheme".into()).spawn(move || portal::follow(&on_change));
    });
}

#[cfg(target_os = "linux")]
mod portal {
    use std::sync::atomic::{AtomicU32, Ordering};

    use zbus::blocking::{Connection, MessageIterator};
    use zbus::zvariant::OwnedValue;

    /// `org.freedesktop.appearance color-scheme`: 0 no preference, 1 prefer dark, 2 prefer light.
    pub static SCHEME: AtomicU32 = AtomicU32::new(0);
    pub const PREFER_LIGHT: u32 = 2;

    const DEST: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const SETTINGS: &str = "org.freedesktop.portal.Settings";

    pub fn follow(on_change: &dyn Fn()) {
        let Some(bus) = crate::platform::dbus::session() else { return };
        let rule = "type='signal',interface='org.freedesktop.portal.Settings',member='SettingChanged',\
                    path='/org/freedesktop/portal/desktop',arg0='org.freedesktop.appearance',arg1='color-scheme'";
        let changes = MessageIterator::for_match_rule(rule, &bus, None);
        if let Some(scheme) = read(&bus) {
            SCHEME.store(scheme, Ordering::Relaxed);
            on_change();
        }
        let Ok(changes) = changes else { return };
        for message in changes.flatten() {
            let body = message.body();
            let Ok((_, _, value)) = body.deserialize::<(String, String, OwnedValue)>() else { continue };
            if let Ok(scheme) = value.downcast_ref::<u32>() {
                SCHEME.store(scheme, Ordering::Relaxed);
                on_change();
            }
        }
    }

    /// The current value: `ReadOne` (portal 2+), else the older `Read`, whose answer is wrapped in one more variant.
    fn read(bus: &Connection) -> Option<u32> {
        let args = ("org.freedesktop.appearance", "color-scheme");
        let reply = bus
            .call_method(Some(DEST), PATH, Some(SETTINGS), "ReadOne", &args)
            .or_else(|_| bus.call_method(Some(DEST), PATH, Some(SETTINGS), "Read", &args))
            .ok()?;
        let value: OwnedValue = reply.body().deserialize().ok()?;
        value.downcast_ref::<u32>().ok()
    }
}
