#![windows_subsystem = "windows"]
//! ThinkVision Control look-alike in Slint, with a background-mode prototype. Static/fake data only.
//!
//! Window flags: --dark | --light (force the theme; default follows the OS), --monitor (open on Monitor settings),
//!   --exact-accent (Windows: draw the accent with the exact WinUI shades from the AccentPalette registry value).
//! Background mode (tray icon + global hotkeys + OSD), enabled by either:
//!   --startup     start in the tray, window not shown
//!   --tray        show the window, but keep running in the tray when it is closed
//!   --close=hide  (default) closing hides the window and keeps the component; the OSD window is kept and hidden
//!   --close=drop  closing drops the window component (recreated on open); the OSD is dropped after auto-hide
//!   --log=<path>  append timestamped events (QueryPerformanceCounter ticks on Windows) for latency measurement
//! Hotkeys: Ctrl+Alt+Up / Ctrl+Alt+Down step the fake brightness by 5, Ctrl+Alt+O opens the window.

slint::include_modules!();

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::time::Duration;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use slint::winit_030::winit::window::{Theme, WindowAttributes};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

const OSD_TITLE: &str = "ThinkVision OSD";

#[derive(Clone, Copy, PartialEq)]
enum Close {
    Hide,
    Drop,
}

struct Opts {
    scheme: i32, // 0 = follow OS, 1 = dark, 2 = light
    page: i32,
    exact_accent: bool,
    startup: bool,
    tray: bool,
    close: Close,
    log: Option<String>,
}

struct App {
    opts: Opts,
    palette: Option<[slint::Color; 8]>,
    main: RefCell<Option<MainWindow>>,
    osd: RefCell<Option<Osd>>,
    osd_shown: Cell<bool>,
    osd_timer: slint::Timer,
    brightness: Cell<i32>,
    hotkeys: [u32; 3], // up, down, open
    log: RefCell<Option<std::fs::File>>,
}

impl App {
    fn log(&self, line: std::fmt::Arguments) {
        if let Some(f) = self.log.borrow_mut().as_mut() {
            let _ = writeln!(f, "{line}");
        }
    }
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

fn app() -> Rc<App> {
    APP.with(|a| a.borrow().clone().expect("app state"))
}

fn parse() -> Opts {
    let mut o = Opts { scheme: 0, page: 0, exact_accent: false, startup: false, tray: false, close: Close::Hide, log: None };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dark" => o.scheme = 1,
            "--light" => o.scheme = 2,
            "--monitor" => o.page = 1,
            "--exact-accent" => o.exact_accent = true,
            "--startup" => o.startup = true,
            "--tray" => o.tray = true,
            "--close=hide" => o.close = Close::Hide,
            "--close=drop" => o.close = Close::Drop,
            a if a.starts_with("--log=") => o.log = Some(a["--log=".len()..].to_string()),
            _ => {}
        }
    }
    o
}

fn main() -> Result<(), slint::PlatformError> {
    let opts = parse();

    // One winit hook for every window Slint creates:
    // - the forced theme, so the native title bar matches --dark / --light;
    // - for the OSD: never activate on show, and no taskbar button.
    let theme = match opts.scheme {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    };
    slint::BackendSelector::new()
        .with_winit_window_attributes_hook(move |attrs: WindowAttributes| {
            let is_osd = attrs.title == OSD_TITLE;
            let attrs = if theme.is_some() { attrs.with_theme(theme) } else { attrs };
            if is_osd {
                osd_attributes(attrs)
            } else {
                attrs
            }
        })
        .select()?;

    let palette = if opts.exact_accent { accent_palette() } else { None };

    if !(opts.startup || opts.tray) {
        // Plain look-alike: one window, closing it quits.
        let ui = MainWindow::new()?;
        init_main(&ui, &opts, palette);
        return ui.run();
    }

    // ---- background mode ----
    let manager = GlobalHotKeyManager::new().expect("global hotkey manager");
    let mods = Some(Modifiers::CONTROL | Modifiers::ALT);
    let keys = [HotKey::new(mods, Code::ArrowUp), HotKey::new(mods, Code::ArrowDown), HotKey::new(mods, Code::KeyO)];
    let mut failed = Vec::new();
    for k in keys {
        if let Err(e) = manager.register(k) {
            failed.push(format!("{k:?}: {e}"));
        }
    }

    let log = opts.log.as_ref().and_then(|p| std::fs::OpenOptions::new().create(true).append(true).open(p).ok());
    let app = Rc::new(App {
        opts,
        palette,
        main: RefCell::new(None),
        osd: RefCell::new(None),
        osd_shown: Cell::new(false),
        osd_timer: slint::Timer::default(),
        brightness: Cell::new(0),
        hotkeys: [keys[0].id(), keys[1].id(), keys[2].id()],
        log: RefCell::new(log),
    });
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));
    for f in &failed {
        app.log(format_args!("{} hotkey-register-failed {f}", ticks()));
    }

    // Hotkeys: global-hotkey calls this from its message window (Windows) or listener thread (Linux). No polling
    // timer: the event is forwarded into Slint's loop, which otherwise sleeps.
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state() == HotKeyState::Pressed {
            let t = ticks();
            let id = e.id();
            let _ = slint::invoke_from_event_loop(move || on_hotkey(id, t));
        }
    }));

    let tray = Tray::new()?;
    tray.on_open(|| open_main(&self::app()));
    tray.on_preset(|i| {
        let a = self::app();
        a.brightness.set(if i == 0 { 80 } else { 20 });
        sync_main(&a);
        show_osd(&a);
    });
    tray.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    if !app.opts.startup {
        open_main(&app);
    }
    app.log(format_args!("{} ready", ticks()));

    slint::run_event_loop_until_quit()?;

    drop(tray);
    drop(manager);
    Ok(())
}

fn init_main(ui: &MainWindow, opts: &Opts, palette: Option<[slint::Color; 8]>) {
    ui.set_forced_scheme(opts.scheme);
    ui.set_page(opts.page);
    if let Some(p) = palette {
        let g = ui.global::<AccentOverride>();
        g.set_on_dark(p[1]); // Light2
        g.set_on_light(p[4]); // Dark1
        g.set_enabled(true);
    }
}

fn on_hotkey(id: u32, t_recv: i64) {
    let a = app();
    if id == a.hotkeys[2] {
        a.log(format_args!("{t_recv} hk-open"));
        open_main(&a);
        return;
    }
    let step = if id == a.hotkeys[0] { 5 } else { -5 };
    a.brightness.set((a.brightness.get() + step).clamp(0, 100));
    a.log(format_args!("{t_recv} hk {}", a.brightness.get()));
    sync_main(&a);
    show_osd(&a);
}

fn sync_main(a: &App) {
    if let Some(ui) = a.main.borrow().as_ref() {
        // Only into a visible window: open_main sets the current value when it shows the window again.
        if ui.window().is_visible() {
            ui.set_brightness(a.brightness.get() as f32);
        }
    }
}

fn open_main(a: &Rc<App>) {
    if a.main.borrow().is_none() {
        let ui = MainWindow::new().expect("main window");
        init_main(&ui, &a.opts, a.palette);
        ui.window().on_close_requested(|| {
            let a = app();
            a.log(format_args!("{} main-close", ticks()));
            if a.opts.close == Close::Drop {
                // Not from inside the component's own callback: drop it on the next loop iteration.
                slint::Timer::single_shot(Duration::ZERO, || {
                    let a = app();
                    let w = a.main.borrow_mut().take();
                    drop(w);
                    a.log(format_args!("{} main-dropped", ticks()));
                });
            }
            slint::CloseRequestResponse::HideWindow
        });
        *a.main.borrow_mut() = Some(ui);
    }
    let main = a.main.borrow();
    let ui = main.as_ref().unwrap();
    ui.set_brightness(a.brightness.get() as f32);
    let _ = ui.show();
    ui.window().with_winit_window(|w| w.focus_window());
    a.log(format_args!("{} main-shown", ticks()));
}

fn show_osd(a: &Rc<App>) {
    let v = a.brightness.get();
    let created = a.osd.borrow().is_none();
    if created {
        let o = Osd::new().expect("osd");
        o.set_value(v);
        place_osd(&o);
        let _ = o.show(); // first show of a with_active(false) winit window = SW_SHOWNOACTIVATE
        *a.osd.borrow_mut() = Some(o);
        // Slint creates the winit window lazily on the next loop iteration, so style it once it exists.
        style_osd_when_ready(0);
    } else {
        let osd = a.osd.borrow();
        let o = osd.as_ref().unwrap();
        o.set_value(v);
        if !a.osd_shown.get() {
            native_show(o, true);
        }
    }
    a.osd_shown.set(true);
    a.log(format_args!("{} osd-shown {} {v}", ticks(), if created { "created" } else { "reused" }));
    a.osd_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(1500), || hide_osd(&app()));
}

fn style_osd_when_ready(attempt: u32) {
    let a = app();
    let ready = match a.osd.borrow().as_ref() {
        Some(o) if window_ready(o.window()) => {
            style_osd(o);
            true
        }
        Some(_) => false,
        None => return,
    };
    if ready {
        a.log(format_args!("{} osd-styled {attempt}", ticks()));
    } else if attempt < 100 {
        slint::Timer::single_shot(Duration::from_millis(1), move || style_osd_when_ready(attempt + 1));
    }
}

fn window_ready(w: &slint::Window) -> bool {
    w.with_winit_window(|_| ()).is_some()
}

fn hide_osd(a: &Rc<App>) {
    if a.opts.close == Close::Drop {
        if let Some(o) = a.osd.borrow_mut().take() {
            let _ = o.hide();
        }
    } else if let Some(o) = a.osd.borrow().as_ref() {
        native_show(o, false);
    }
    a.osd_shown.set(false);
    a.log(format_args!("{} osd-hidden", ticks()));
}

// ---------------------------------------------------------------------------------------------------------------
// Platform glue

#[cfg(windows)]
fn osd_attributes(attrs: WindowAttributes) -> WindowAttributes {
    use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
    attrs.with_active(false).with_skip_taskbar(true)
}

#[cfg(not(windows))]
fn osd_attributes(attrs: WindowAttributes) -> WindowAttributes {
    attrs.with_active(false)
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
    #[link(name = "kernel32")]
    extern "system" {
        pub fn QueryPerformanceCounter(count: *mut i64) -> i32;
    }
}

#[cfg(windows)]
fn hwnd_of(w: &slint::Window) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    w.with_winit_window(|ww| match ww.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    })
    .flatten()
}

/// Bottom centre of the primary work area, 48 px above the taskbar.
#[cfg(windows)]
fn place_osd(o: &Osd) {
    let mut r = [0i32; 4]; // left, top, right, bottom
    if unsafe { win::SystemParametersInfoW(0x0030 /* SPI_GETWORKAREA */, 0, r.as_mut_ptr().cast(), 0) } != 0 {
        let (w, h) = (264, 64);
        o.window().set_position(slint::PhysicalPosition::new((r[0] + r[2]) / 2 - w / 2, r[3] - h - 48));
    }
}

#[cfg(not(windows))]
fn place_osd(_o: &Osd) {}

/// Click-through through winit, then the Win32 bits winit doesn't offer after creation: tool window (not in Alt+Tab),
/// no-activate, opaque layered alpha (winit adds WS_EX_LAYERED for click-through), DWM rounded corners, topmost.
#[cfg(windows)]
fn style_osd(o: &Osd) {
    {
        o.window().with_winit_window(|w| {
            let _ = w.set_cursor_hittest(false);
        });
    }
    if let Some(h) = hwnd_of(o.window()) {
        unsafe {
            const GWL_EXSTYLE: i32 = -20;
            {
                let ex = win::GetWindowLongPtrW(h, GWL_EXSTYLE);
                let ex = (ex | 0x80 /* TOOLWINDOW */ | 0x0800_0000 /* NOACTIVATE */ | 0x8_0000 /* LAYERED */) & !0x4_0000 /* APPWINDOW */;
                win::SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
                win::SetLayeredWindowAttributes(h, 0, 255, 2 /* LWA_ALPHA */);
            }
            {
                let round: u32 = 2; // DWMWCP_ROUND
                win::DwmSetWindowAttribute(h, 33 /* DWMWA_WINDOW_CORNER_PREFERENCE */, (&round as *const u32).cast(), 4);
            }
            {
                win::SetWindowPos(h, -1 /* HWND_TOPMOST */, 0, 0, 0, 0, 0x1 | 0x2 | 0x10 | 0x20); // NOSIZE NOMOVE NOACTIVATE FRAMECHANGED
            }
            app().log(format_args!("{} osd-exstyle 0x{:X}", ticks(), win::GetWindowLongPtrW(h, GWL_EXSTYLE)));
        }
    }
}

#[cfg(not(windows))]
fn style_osd(o: &Osd) {
    o.window().with_winit_window(|w| {
        let _ = w.set_cursor_hittest(false);
    });
}

/// Show / hide a kept OSD by its layered alpha (255 / 0). The window stays "visible" to Slint and winit:
/// - winit's own set_visible(true) after the first show uses SW_SHOW (activates, steals focus) and rewrites
///   GWL_EXSTYLE (drops TOOLWINDOW/NOACTIVATE), so Slint's hide()/show() can't be used for a no-focus OSD;
/// - hiding with Win32 SW_HIDE behind Slint's back made Slint's loop spin on redraws (~8-15 ms CPU per second, measured).
#[cfg(windows)]
fn native_show(o: &Osd, show: bool) {
    if let Some(h) = hwnd_of(o.window()) {
        unsafe { win::SetLayeredWindowAttributes(h, 0, if show { 255 } else { 0 }, 2 /* LWA_ALPHA */) };
    }
}

#[cfg(not(windows))]
fn native_show(o: &Osd, show: bool) {
    let _ = if show { o.show() } else { o.hide() };
}

#[cfg(windows)]
fn ticks() -> i64 {
    let mut t = 0i64;
    unsafe { win::QueryPerformanceCounter(&mut t) };
    t
}

#[cfg(not(windows))]
fn ticks() -> i64 {
    let mut ts = libc_timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { clock_gettime(1 /* CLOCK_MONOTONIC */, &mut ts) };
    ts.tv_sec * 1_000_000_000 + ts.tv_nsec
}

#[cfg(not(windows))]
#[repr(C)]
struct libc_timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[cfg(not(windows))]
extern "C" {
    fn clock_gettime(clk: i32, ts: *mut libc_timespec) -> i32;
}

/// HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent\AccentPalette: 8 RGBA entries
/// (Light3, Light2, Light1, Accent, Dark1, Dark2, Dark3, unused). Read with plain advapi32, no extra crate.
#[cfg(windows)]
fn accent_palette() -> Option<[slint::Color; 8]> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(hkey: isize, sub: *const u16, value: *const u16, flags: u32, ty: *mut u32, data: *mut u8, len: *mut u32) -> i32;
    }
    const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
    const RRF_RT_REG_BINARY: u32 = 0x8;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, value) = (wide(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent"), wide("AccentPalette"));
    let mut buf = [0u8; 32];
    let mut len = buf.len() as u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), buf.as_mut_ptr(), &mut len)
    };
    if rc != 0 || len != 32 {
        return None;
    }
    Some(std::array::from_fn(|i| slint::Color::from_rgb_u8(buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2])))
}

#[cfg(not(windows))]
fn accent_palette() -> Option<[slint::Color; 8]> {
    None
}
