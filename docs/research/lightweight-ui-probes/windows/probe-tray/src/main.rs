#![windows_subsystem = "windows"]
// Tray-only probe: tray icon with a 2-item menu + one global hotkey (Ctrl+Alt+M). No window.
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};

#[repr(C)]
struct Msg { hwnd: isize, message: u32, wparam: usize, lparam: isize, time: u32, x: i32, y: i32 }
#[link(name = "user32")]
extern "system" {
    fn GetMessageW(m: *mut Msg, h: isize, a: u32, b: u32) -> i32;
    fn TranslateMessage(m: *const Msg) -> i32;
    fn DispatchMessageW(m: *const Msg) -> isize;
}

fn main() {
    let menu = Menu::new();
    let open = MenuItem::new("Open", true, None);
    let exit = MenuItem::new("Exit", true, None);
    menu.append_items(&[&open, &exit]).unwrap();
    let exit_id = exit.id().clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| if e.id == exit_id { std::process::exit(0) }));

    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..256 { rgba.extend_from_slice(&[0xF0, 0x5A, 0x28, 0xFF]); }
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("probe")
        .with_icon(Icon::from_rgba(rgba, 16, 16).unwrap())
        .build()
        .unwrap();

    let hk = GlobalHotKeyManager::new().unwrap();
    hk.register(HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyM)).unwrap();
    GlobalHotKeyEvent::set_event_handler(Some(|_e: GlobalHotKeyEvent| {}));

    let mut m = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, x: 0, y: 0 };
    unsafe {
        while GetMessageW(&mut m, 0, 0, 0) > 0 { TranslateMessage(&m); DispatchMessageW(&m); }
    }
}
