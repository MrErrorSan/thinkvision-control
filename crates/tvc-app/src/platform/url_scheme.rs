//! `thinkvision://` links. The installer, `.desktop` file and Info.plist register the scheme; on Windows the
//! app also registers itself per user, so a portable copy (or a moved install) still opens links.

/// Windows: `HKCU\Software\Classes\thinkvision` → `"<exe>" "%1"`, writing only values that differ (idempotent;
/// best effort, errors ignored). No-op elsewhere.
pub fn register() {
    imp::register();
}

const ROOT: &str = r"Software\Classes\thinkvision";

/// The values to set, as (subkey under HKCU, value name — `None` for the default value, data).
fn entries(exe: &str) -> [(String, Option<&'static str>, String); 4] {
    [
        (ROOT.into(), None, "URL:ThinkVision Control".into()),
        (ROOT.into(), Some("URL Protocol"), String::new()),
        (format!(r"{ROOT}\DefaultIcon"), None, format!("\"{exe}\",0")),
        (format!(r"{ROOT}\shell\open\command"), None, format!("\"{exe}\" \"%1\"")),
    ]
}

#[cfg(windows)]
mod imp {
    use crate::platform::wide;
    use windows_sys::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

    pub fn register() {
        let Ok(exe) = std::env::current_exe() else { return };
        for (key, name, data) in super::entries(&exe.display().to_string()) {
            let key = wide(&key);
            let name = name.map(wide);
            let name_ptr = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
            if read(&key, name_ptr).as_deref() == Some(data.as_str()) {
                continue;
            }
            let data = wide(&data);
            // SAFETY: NUL-terminated strings (a null name sets the default value); the byte count covers `data`
            // including its NUL. Missing subkeys are created.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name_ptr,
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
        }
    }

    /// A REG_SZ value under HKCU, or None when missing or not a string.
    fn read(key: &[u16], name: *const u16) -> Option<String> {
        let mut bytes = 0u32;
        // SAFETY: a size query (null buffer), then a read into a buffer of exactly that many bytes.
        unsafe {
            let (flags, no_type) = (RRF_RT_REG_SZ, std::ptr::null_mut());
            let size_only = std::ptr::null_mut();
            if RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name, flags, no_type, size_only, &mut bytes) != 0 {
                return None;
            }
            let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
            let data = buf.as_mut_ptr().cast();
            let status = RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name, flags, no_type, data, &mut bytes);
            if status != 0 {
                return None;
            }
            buf.truncate(bytes as usize / 2);
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn register() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_point_at_the_exe() {
        let e = entries(r"C:\Program Files\ThinkVision Control\thinkvision-control.exe");
        assert_eq!(e[0], (ROOT.to_string(), None, "URL:ThinkVision Control".to_string()));
        assert_eq!(e[1], (ROOT.to_string(), Some("URL Protocol"), String::new()));
        assert_eq!(e[2].2, r#""C:\Program Files\ThinkVision Control\thinkvision-control.exe",0"#);
        assert_eq!(e[3].0, r"Software\Classes\thinkvision\shell\open\command");
        assert_eq!(e[3].2, r#""C:\Program Files\ThinkVision Control\thinkvision-control.exe" "%1""#);
    }
}
