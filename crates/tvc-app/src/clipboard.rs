//! Copy text for the setup hints' Copy buttons. winit has no clipboard; Windows uses Win32, Linux the arboard
//! clipboard Slint's winit backend uses too (Wayland data-control, else X11/XWayland), with the desktop's own tool
//! (wl-copy, xclip, xsel) as the fallback, and macOS pbcopy.

#[cfg(windows)]
pub fn set(text: &str) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(owner: isize) -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, mem: isize) -> isize;
        fn CloseClipboard() -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalLock(mem: isize) -> *mut u16;
        fn GlobalUnlock(mem: isize) -> i32;
        fn GlobalFree(mem: isize) -> isize;
    }
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x2;
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: standard clipboard sequence; the buffer is sized for `wide` and ownership passes to the clipboard
    // on success (freed here only if SetClipboardData fails).
    unsafe {
        if OpenClipboard(0) == 0 {
            return false;
        }
        EmptyClipboard();
        let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        let ok = mem != 0 && {
            let p = GlobalLock(mem);
            !p.is_null() && {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                GlobalUnlock(mem);
                SetClipboardData(CF_UNICODETEXT, mem) != 0
            }
        };
        if !ok && mem != 0 {
            GlobalFree(mem);
        }
        CloseClipboard();
        ok
    }
}

#[cfg(not(windows))]
pub fn set(text: &str) -> bool {
    #[cfg(target_os = "linux")]
    if set_arboard(text) {
        return true;
    }
    set_with_tool(text)
}

/// Through arboard. The clipboard object is kept for the life of the UI thread: on X11 the copied text is served
/// only while one exists.
#[cfg(target_os = "linux")]
fn set_arboard(text: &str) -> bool {
    thread_local! {
        static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> = const { std::cell::RefCell::new(None) };
    }
    CLIPBOARD.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = arboard::Clipboard::new().ok();
        }
        slot.as_mut().is_some_and(|c| c.set_text(text).is_ok())
    })
}

#[cfg(not(windows))]
fn set_with_tool(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else {
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"]), ("xsel", &["--clipboard", "--input"])]
    };
    tools.iter().any(|(tool, args)| {
        let Ok(mut child) = Command::new(tool).args(*args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
            return false;
        };
        let wrote = child.stdin.take().is_some_and(|mut s| s.write_all(text.as_bytes()).is_ok());
        wrote && child.wait().is_ok_and(|s| s.success())
    })
}
