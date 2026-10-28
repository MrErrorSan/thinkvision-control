//! Global hotkeys through the `global-hotkey` crate: Windows `RegisterHotKey`, X11 key grabs, macOS Carbon
//!. Wayland sessions report unavailable until the xdg-desktop-portal follow-up.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashMap;
use std::sync::Mutex;

/// A shortcut as 0.0.4 stores it: Win32 MOD bits (Alt 1, Ctrl 2, Shift 4, Win 8) and a Win32 virtual-key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gesture {
    pub mods: u8,
    pub vk: u16,
}

const MOD_ALT: u8 = 1;
const MOD_CONTROL: u8 = 2;
const MOD_SHIFT: u8 = 4;
const MOD_WIN: u8 = 8;

/// The press callback and the crate's hotkey id → our action id. The crate's event handler is process-global and
/// can be set only once, so it reads from here.
struct Route {
    on_press: Box<dyn Fn(usize) + Send>,
    actions: HashMap<u32, usize>,
}

static ROUTE: Mutex<Option<Route>> = Mutex::new(None);

fn route() -> std::sync::MutexGuard<'static, Option<Route>> {
    ROUTE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The registered set. Dropping it unregisters everything.
pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    active: Vec<HotKey>,
}

impl Hotkeys {
    /// None when this session can't register global hotkeys (Wayland, no X display, or the OS refused).
    ///
    /// **Create it on the thread that runs the Slint event loop**: on Windows the crate's hidden window receives
    /// `WM_HOTKEY` through that thread's message loop, and on macOS Carbon delivers to the main run loop.
    /// `on_press(action)` runs on that thread (Windows, macOS) or the crate's X11 thread (Linux); forward it to the
    /// UI with `slint::invoke_from_event_loop` and never call back into `Hotkeys` from inside it. One at a time:
    /// drop the old one before creating another.
    pub fn new(on_press: impl Fn(usize) + Send + 'static) -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            let var = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
            let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland") || var("WAYLAND_DISPLAY");
            // On XWayland the grabs only fire while an X11 window has focus, so they'd look registered but stay
            // dead; without a display the crate's X11 thread exits and registrations "succeed" silently.
            if wayland || !var("DISPLAY") {
                return None;
            }
        }
        let manager = GlobalHotKeyManager::new().ok()?;
        *route() = Some(Route { on_press: Box::new(on_press), actions: HashMap::new() });
        GlobalHotKeyEvent::set_event_handler(Some(|e: GlobalHotKeyEvent| {
            if e.state() != HotKeyState::Pressed {
                return;
            }
            if let Some(r) = route().as_ref() {
                if let Some(&action) = r.actions.get(&e.id()) {
                    (r.on_press)(action);
                }
            }
        }));
        Some(Self { manager, active: Vec::new() })
    }

    /// Replaces the whole registered set with `map` and returns the actions that failed: another app owns the
    /// combination, the gesture is invalid or unsupported here (macOS: no Command/Control, or F21–F24), or it
    /// duplicates an earlier entry. Leave unset gestures out of `map`.
    pub fn register(&mut self, map: &[(usize, Gesture)]) -> Vec<usize> {
        self.suspend();
        let mut failed = Vec::new();
        let mut actions = HashMap::new();
        for &(action, gesture) in map {
            let Some(hotkey) = to_hotkey(gesture).filter(|_| allowed_here(gesture)) else {
                failed.push(action);
                continue;
            };
            if actions.contains_key(&hotkey.id()) || self.manager.register(hotkey).is_err() {
                failed.push(action);
                continue;
            }
            actions.insert(hotkey.id(), action);
            self.active.push(hotkey);
        }
        if let Some(r) = route().as_mut() {
            r.actions = actions;
        }
        failed
    }

    /// Unregisters everything (shortcut editor open, background mode off). `register` brings them back.
    pub fn suspend(&mut self) {
        for hotkey in self.active.drain(..) {
            let _ = self.manager.unregister(hotkey);
        }
        if let Some(r) = route().as_mut() {
            r.actions.clear();
        }
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.suspend();
    }
}

fn allowed_here(g: Gesture) -> bool {
    !cfg!(target_os = "macos") || mac_allows(g)
}

/// macOS 15 refuses hotkeys whose only modifiers are Shift/Option, so Command (Win) or Control is required; applied
/// on every version so a shortcut keeps working after an upgrade. macOS has no F21–F24.
fn mac_allows(g: Gesture) -> bool {
    g.mods & (MOD_CONTROL | MOD_WIN) != 0 && !(0x84..=0x87).contains(&g.vk)
}

/// Win32 MOD + VK → the crate's hotkey. None for no modifier, unknown modifier bits or an unsupported key.
fn to_hotkey(g: Gesture) -> Option<HotKey> {
    if g.mods == 0 || g.mods & !0x0F != 0 {
        return None;
    }
    let mut mods = Modifiers::empty();
    for (bit, m) in [
        (MOD_ALT, Modifiers::ALT),
        (MOD_CONTROL, Modifiers::CONTROL),
        (MOD_SHIFT, Modifiers::SHIFT),
        (MOD_WIN, Modifiers::META),
    ] {
        if g.mods & bit != 0 {
            mods |= m;
        }
    }
    Some(HotKey::new(Some(mods), key_code(g.vk)?))
}

/// The keys 0.0.4's gesture format allows: A–Z, 0–9, F1–F24, PageUp/Down, End, Home, arrows, Insert, Delete.
fn key_code(vk: u16) -> Option<Code> {
    use Code::*;
    const LETTERS: [Code; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR,
        KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [Code; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    const FKEYS: [Code; 24] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24,
    ];
    Some(match vk {
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x70..=0x87 => FKEYS[(vk - 0x70) as usize],
        0x21 => PageUp,
        0x22 => PageDown,
        0x23 => End,
        0x24 => Home,
        0x25 => ArrowLeft,
        0x26 => ArrowUp,
        0x27 => ArrowRight,
        0x28 => ArrowDown,
        0x2D => Insert,
        0x2E => Delete,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys() {
        assert_eq!(key_code(0x41), Some(Code::KeyA));
        assert_eq!(key_code(0x5A), Some(Code::KeyZ));
        assert_eq!(key_code(0x30), Some(Code::Digit0));
        assert_eq!(key_code(0x39), Some(Code::Digit9));
        assert_eq!(key_code(0x70), Some(Code::F1));
        assert_eq!(key_code(0x87), Some(Code::F24));
        assert_eq!(key_code(0x21), Some(Code::PageUp));
        assert_eq!(key_code(0x28), Some(Code::ArrowDown));
        assert_eq!(key_code(0x2E), Some(Code::Delete));
        assert_eq!(key_code(0x20), None); // Space isn't in the gesture format
        assert_eq!(key_code(0x88), None);
    }

    #[test]
    fn maps_modifiers() {
        let hk = to_hotkey(Gesture { mods: MOD_CONTROL | MOD_ALT, vk: 0x26 }).unwrap();
        assert_eq!(hk, HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowUp));
        let hk = to_hotkey(Gesture { mods: MOD_SHIFT | MOD_WIN, vk: 0x4F }).unwrap();
        assert_eq!(hk, HotKey::new(Some(Modifiers::SHIFT | Modifiers::META), Code::KeyO));
        assert!(to_hotkey(Gesture { mods: 0, vk: 0x41 }).is_none());
        assert!(to_hotkey(Gesture { mods: 0x10 | MOD_CONTROL, vk: 0x41 }).is_none());
        assert!(to_hotkey(Gesture { mods: MOD_CONTROL, vk: 0x20 }).is_none());
    }

    #[test]
    fn mac_rule() {
        assert!(mac_allows(Gesture { mods: MOD_CONTROL | MOD_ALT, vk: 0x26 }));
        assert!(mac_allows(Gesture { mods: MOD_WIN, vk: 0x41 }));
        assert!(!mac_allows(Gesture { mods: MOD_ALT | MOD_SHIFT, vk: 0x26 }));
        assert!(!mac_allows(Gesture { mods: MOD_CONTROL, vk: 0x84 })); // F21
        assert!(mac_allows(Gesture { mods: MOD_CONTROL, vk: 0x83 })); // F20
    }
}
