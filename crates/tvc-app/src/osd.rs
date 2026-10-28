//! The step OSD. One window, created on first use and then kept: it stays "visible" to Slint and winit and
//! is shown and hidden by layered alpha on Windows. winit's own show after the first one activates the window and
//! rewrites its ex-styles, and hiding through Win32 behind Slint's back makes Slint's loop spin (background NOTES).

use std::time::Duration;

use slint::winit_030::winit::window::WindowAttributes;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use crate::Osd;

pub const TITLE: &str = "ThinkVision OSD";
const VISIBLE_FOR: Duration = Duration::from_millis(1200);
const WIDTH: f32 = 320.0;
const HEIGHT: f32 = 64.0;
const ABOVE_WORK_AREA: f32 = 96.0;

/// Window attributes for the OSD: never activated on show, no taskbar button. Called from the winit hook.
pub fn attributes(attrs: WindowAttributes) -> WindowAttributes {
    #[cfg(windows)]
    {
        use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_active(false).with_skip_taskbar(true)
    }
    #[cfg(not(windows))]
    attrs.with_active(false)
}

pub enum Content<'a> {
    Level { glyph: &'a str, label: &'a str, value: u16, max: u16 },
    Message(&'a str),
}

#[derive(Default)]
pub struct OsdHost {
    window: Option<Osd>,
    shown: bool,
    timer: slint::Timer,
}

impl OsdHost {
    /// Shows or updates the OSD; it hides 1.2 s after the last call.
    pub fn show(this: &std::rc::Rc<std::cell::RefCell<Self>>, content: Content) {
        let mut host = this.borrow_mut();
        let created = host.window.is_none();
        if created {
            let Ok(osd) = Osd::new() else { return };
            // The kept OSD stays "visible" (alpha 0), so Windows may pick it as the process's main window. It can't
            // be clicked or focused, so a close request can only come from outside (installer upgrade, taskkill
            // without /F): exit in order rather than hide it behind Slint's back.
            osd.window().on_close_requested(|| {
                let _ = slint::quit_event_loop();
                slint::CloseRequestResponse::KeepWindowShown
            });
            host.window = Some(osd);
        }
        let osd = host.window.as_ref().unwrap();
        match content {
            Content::Level { glyph, label, value, max } => {
                osd.set_message("".into());
                osd.set_glyph(glyph.into());
                osd.set_label(label.into());
                osd.set_value_text(value.to_string().into());
                osd.set_fraction(if max == 0 { 0.0 } else { value.min(max) as f32 / max as f32 });
            }
            Content::Message(text) => osd.set_message(text.into()),
        }
        if created {
            place(osd);
            let _ = osd.show(); // first show of a with_active(false) window = no activation
            // Not called directly: `host` is still borrowed here.
            let weak = std::rc::Rc::downgrade(this);
            slint::Timer::single_shot(Duration::ZERO, move || style_when_ready(weak, 0));
        } else if !host.shown {
            place(osd); // the work area may have changed since the last show
            native_show(osd, true);
        }
        host.shown = true;
        let weak = std::rc::Rc::downgrade(this);
        host.timer.start(slint::TimerMode::SingleShot, VISIBLE_FOR, move || {
            if let Some(this) = weak.upgrade() {
                let mut host = this.borrow_mut();
                if let Some(osd) = host.window.as_ref() {
                    native_show(osd, false);
                }
                host.shown = false;
            }
        });
    }
}

// Slint creates the native window on the next loop iteration, so style it once it exists.
fn style_when_ready(weak: std::rc::Weak<std::cell::RefCell<OsdHost>>, attempt: u32) {
    let Some(this) = weak.upgrade() else { return };
    let host = this.borrow();
    let Some(osd) = host.window.as_ref() else { return };
    if osd.window().with_winit_window(|_| ()).is_some() {
        style(osd);
    } else if attempt < 100 {
        slint::Timer::single_shot(Duration::from_millis(2), move || style_when_ready(weak, attempt + 1));
    }
}

#[cfg(windows)]
mod win {
    #[link(name = "user32")]
    extern "system" {
        pub fn GetWindowLongPtrW(h: isize, i: i32) -> isize;
        pub fn SetWindowLongPtrW(h: isize, i: i32, v: isize) -> isize;
        pub fn SetWindowPos(h: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        pub fn SetLayeredWindowAttributes(h: isize, key: u32, alpha: u8, flags: u32) -> i32;
        pub fn SystemParametersInfoW(action: u32, param: u32, pv: *mut core::ffi::c_void, ini: u32) -> i32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmSetWindowAttribute(h: isize, attr: u32, value: *const core::ffi::c_void, size: u32) -> i32;
    }
}

#[cfg(windows)]
fn hwnd(osd: &Osd) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    osd.window()
        .with_winit_window(|w| match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        })
        .flatten()
}

/// Bottom centre of the primary work area, the card's bottom edge 96 px above it.
#[cfg(windows)]
fn place(osd: &Osd) {
    let mut r = [0i32; 4]; // left, top, right, bottom (physical px)
    // SAFETY: SPI_GETWORKAREA writes one RECT into `r`.
    if unsafe { win::SystemParametersInfoW(0x0030, 0, r.as_mut_ptr().cast(), 0) } != 0 {
        let scale = osd.window().scale_factor();
        let (w, h, gap) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32, (ABOVE_WORK_AREA * scale) as i32);
        osd.window().set_position(slint::PhysicalPosition::new((r[0] + r[2]) / 2 - w / 2, r[3] - h - gap));
    }
}

#[cfg(not(windows))]
fn place(osd: &Osd) {
    // winit offers no work area; centre near the bottom of the primary monitor when known.
    osd.window().with_winit_window(|w| {
        if let Some(m) = w.primary_monitor().or_else(|| w.current_monitor()) {
            let (s, p, scale) = (m.size(), m.position(), m.scale_factor() as f32);
            let (w_px, h_px, gap) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32, (ABOVE_WORK_AREA * scale) as i32);
            w.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(
                p.x + s.width as i32 / 2 - w_px / 2,
                p.y + s.height as i32 - h_px - gap,
            ));
        }
    });
}

/// Click-through, tool window (not in Alt+Tab), no-activate, opaque layered alpha, rounded corners, topmost.
#[cfg(windows)]
fn style(osd: &Osd) {
    osd.window().with_winit_window(|w| {
        let _ = w.set_cursor_hittest(false);
    });
    let Some(h) = hwnd(osd) else { return };
    const GWL_EXSTYLE: i32 = -20;
    // SAFETY: `h` is the live HWND of the OSD window, owned by this thread.
    unsafe {
        let ex = win::GetWindowLongPtrW(h, GWL_EXSTYLE);
        let ex = (ex | 0x80 /* TOOLWINDOW */ | 0x0800_0000 /* NOACTIVATE */ | 0x8_0000 /* LAYERED */) & !0x4_0000 /* APPWINDOW */;
        win::SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
        win::SetLayeredWindowAttributes(h, 0, 255, 2 /* LWA_ALPHA */);
        let round: u32 = 2; // DWMWCP_ROUND
        win::DwmSetWindowAttribute(h, 33 /* DWMWA_WINDOW_CORNER_PREFERENCE */, (&round as *const u32).cast(), 4);
        win::SetWindowPos(h, -1 /* HWND_TOPMOST */, 0, 0, 0, 0, 0x1 | 0x2 | 0x10 | 0x20);
    }
}

#[cfg(not(windows))]
fn style(osd: &Osd) {
    osd.window().with_winit_window(|w| {
        let _ = w.set_cursor_hittest(false);
    });
}

#[cfg(windows)]
fn native_show(osd: &Osd, show: bool) {
    if let Some(h) = hwnd(osd) {
        // SAFETY: `h` is the live HWND of the OSD window; LWA_ALPHA only changes its opacity.
        unsafe { win::SetLayeredWindowAttributes(h, 0, if show { 255 } else { 0 }, 2) };
    }
}

// Elsewhere winit's show/hide doesn't steal focus the same way; use Slint's own.
#[cfg(not(windows))]
fn native_show(osd: &Osd, show: bool) {
    let _ = if show { osd.show() } else { osd.hide() };
}
