//! Start with the OS. Re-applied on every settings save, so `apply` must be idempotent.

use std::io;

/// The settings row label.
pub fn label() -> &'static str {
    if cfg!(windows) {
        "Start with Windows"
    } else {
        "Start when you log in"
    }
}

/// Turns start-at-login on or off. Windows: HKCU Run value `ThinkVisionControl` = `"<exe>" --startup`.
/// Linux: `~/.config/autostart/thinkvision-control.desktop`. macOS: `SMAppService.mainAppService` (13+, only from
/// a.app bundle; opens System Settings → Login Items while approval is pending).
pub fn apply(enabled: bool) -> io::Result<()> {
    imp::apply(enabled)
}

#[cfg(windows)]
mod imp {
    use crate::platform::wide;
    use std::io;
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "ThinkVisionControl";

    pub fn apply(enabled: bool) -> io::Result<()> {
        let key = wide(RUN_KEY);
        let name = wide(VALUE);
        let status = if enabled {
            let exe = std::env::current_exe()?;
            let data = wide(&format!("\"{}\" --startup", exe.display()));
            // SAFETY: NUL-terminated strings; the byte count covers `data` including its NUL.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            }
        } else {
            // SAFETY: NUL-terminated strings.
            match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => 0,
                status => status,
            }
        };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::{fs, io};

    pub fn apply(enabled: bool) -> io::Result<()> {
        let path = crate::platform::config_home().join("autostart").join("thinkvision-control.desktop");
        if !enabled {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
        }
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or("thinkvision-control".into());
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, super::desktop_entry(&exe))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_void, CStr};
    use std::io;

    type Id = *mut c_void;
    type Sel = *const c_void;

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
        fn objc_autoreleasePoolPush() -> *mut c_void;
        fn objc_autoreleasePoolPop(pool: *mut c_void);
    }
    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    // SMAppServiceStatus (NSInteger).
    const ENABLED: isize = 1;
    const REQUIRES_APPROVAL: isize = 2;

    fn sel(name: &CStr) -> Sel {
        // SAFETY: a NUL-terminated selector name; the runtime interns it.
        unsafe { sel_registerName(name.as_ptr()) }
    }

    // SAFETY (all three): objc_msgSend called through the exact C signature of the selector being sent.
    unsafe fn send_id(obj: Id, s: Sel) -> Id {
        let f: unsafe extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
        f(obj, s)
    }
    unsafe fn send_int(obj: Id, s: Sel) -> isize {
        let f: unsafe extern "C" fn(Id, Sel) -> isize = std::mem::transmute(objc_msgSend as *const ());
        f(obj, s)
    }
    unsafe fn send_bool_err(obj: Id, s: Sel) -> bool {
        let f: unsafe extern "C" fn(Id, Sel, *mut Id) -> u8 = std::mem::transmute(objc_msgSend as *const ());
        let mut error: Id = std::ptr::null_mut();
        f(obj, s, &mut error) != 0
    }

    pub fn apply(enabled: bool) -> io::Result<()> {
        // SMAppService registers the running bundle; a bare binary has nothing to register.
        let in_bundle = std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"));
        if !in_bundle {
            return Ok(());
        }
        // SAFETY: a pool for the autoreleased NSError, popped on every path.
        unsafe {
            let pool = objc_autoreleasePoolPush();
            let result = apply_in_pool(enabled);
            objc_autoreleasePoolPop(pool);
            result
        }
    }

    fn apply_in_pool(enabled: bool) -> io::Result<()> {
        // SAFETY: class and selectors are looked up by name; a nil class (macOS < 13) is checked before use.
        unsafe {
            let class = objc_getClass(c"SMAppService".as_ptr());
            if class.is_null() {
                return Err(io::Error::new(io::ErrorKind::Unsupported, "SMAppService needs macOS 13"));
            }
            let service = send_id(class, sel(c"mainAppService"));
            if service.is_null() {
                return Err(io::Error::other("SMAppService.mainAppService is nil"));
            }
            let status = send_int(service, sel(c"status"));
            let ok = if enabled {
                if status == ENABLED {
                    return Ok(());
                }
                let ok = send_bool_err(service, sel(c"registerAndReturnError:"));
                if send_int(service, sel(c"status")) == REQUIRES_APPROVAL {
                    send_id(class, sel(c"openSystemSettingsLoginItems"));
                }
                ok
            } else if status == ENABLED || status == REQUIRES_APPROVAL {
                send_bool_err(service, sel(c"unregisterAndReturnError:"))
            } else {
                true
            };
            if ok {
                Ok(())
            } else {
                Err(io::Error::other("SMAppService register/unregister failed"))
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub fn apply(_: bool) -> std::io::Result<()> {
        Ok(())
    }
}

/// The XDG autostart entry for `exe` (`LinuxAutostart.Entry` in 0.0.4).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=ThinkVision Control\n\
         Comment=Monitor brightness, contrast and input in the tray\n\
         Exec={} --startup\nTryExec={}\nIcon=thinkvision-control\nTerminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        quote_exec_argument(exe),
        escape_string(exe)
    )
}

/// One Exec argument per the Desktop Entry spec: double-quoted with `" ` $ \` backslash-escaped, then the general
/// string escaping on top (a literal backslash becomes four), and `%` doubled.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn quote_exec_argument(arg: &str) -> String {
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    escape_string(&quoted).replace('%', "%%")
}

/// The general escaping for desktop-file string values.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn escape_string(value: &str) -> String {
    value.replace('\\', r"\\").replace('\n', r"\n").replace('\t', r"\t").replace('\r', r"\r")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_argument_is_quoted_and_escaped() {
        assert_eq!(quote_exec_argument("/opt/tvc/thinkvision-control"), r#""/opt/tvc/thinkvision-control""#);
        assert_eq!(quote_exec_argument("/home/a b/tvc"), r#""/home/a b/tvc""#);
        assert_eq!(quote_exec_argument(r#"/x/"$`"#), r#""/x/\\"\\$\\`""#);
        assert_eq!(quote_exec_argument(r"/x\y"), r#""/x\\\\y""#);
        assert_eq!(quote_exec_argument("/x/100%"), r#""/x/100%%""#);
    }

    #[test]
    fn entry_has_exec_and_tryexec() {
        let entry = desktop_entry("/home/me/My Apps/thinkvision-control");
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nExec=\"/home/me/My Apps/thinkvision-control\" --startup\n"));
        assert!(entry.contains("\nTryExec=/home/me/My Apps/thinkvision-control\n"));
        assert!(entry.contains("\nIcon=thinkvision-control\n"));
        assert!(entry.ends_with("X-GNOME-Autostart-enabled=true\n"));
    }

    #[test]
    fn label_names_the_os() {
        assert_eq!(label(), if cfg!(windows) { "Start with Windows" } else { "Start when you log in" });
    }
}
