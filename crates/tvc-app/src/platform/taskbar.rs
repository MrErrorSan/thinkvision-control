//! Taskbar theme, which picks the tray icon variant (`Tray.OnLight` / `Tray.OnDark`).

/// True when the Windows taskbar is light (`SystemUsesLightTheme`; the taskbar follows "Windows mode", not the
/// app mode). Always false elsewhere.
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
    #[cfg(not(windows))]
    false
}
