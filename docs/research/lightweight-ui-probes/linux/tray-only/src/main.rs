// Tray-only probe: tray icon + 2-item menu + one global hotkey (Ctrl+Alt+B), no window.
// On Linux tray-icon/muda need a GTK 3 main loop; the hotkey uses X11 XGrabKey.
// Prints "ready" once the tray icon and hotkey are registered (used for startup timing).
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIconBuilder,
};

fn main() {
    gtk::init().expect("gtk init");

    let menu = Menu::new();
    let open = MenuItem::new("Open", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&open, &quit]).unwrap();

    // 32x32 solid red icon.
    let rgba: Vec<u8> = (0..32 * 32).flat_map(|_| [0xE2, 0x23, 0x1A, 0xFF]).collect();
    let icon = Icon::from_rgba(rgba, 32, 32).unwrap();
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("probe")
        .with_icon(icon)
        .build()
        .unwrap();

    let manager = GlobalHotKeyManager::new().unwrap();
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyB);
    manager.register(hotkey).unwrap();

    println!("ready");

    let quit_id = quit.id().clone();
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == quit_id {
                gtk::main_quit();
            }
        }
        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            println!("hotkey {:?}", ev);
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}
