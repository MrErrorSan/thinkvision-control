//! Global-shortcut gestures ("Ctrl+Alt+PageUp") and the 11 shortcut actions.

use std::fmt;

/// A global hotkey as Win32 `RegisterHotKey` wants it: `MOD_*` bits plus a virtual-key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gesture {
    pub mods: u8,
    pub vk: u16,
}

impl Gesture {
    pub const MOD_ALT: u8 = 0x1;
    pub const MOD_CONTROL: u8 = 0x2;
    pub const MOD_SHIFT: u8 = 0x4;
    pub const MOD_WIN: u8 = 0x8;
    const ALL_MODS: u8 = 0xF;

    /// Modifiers Ctrl/Control/Alt/Shift/Win (any case, any order) and exactly one key; at least one modifier.
    /// Keys: A–Z, 0–9, F1–F24, PageUp/PgUp, PageDown/PgDn, End, Home, Left, Up, Right, Down, Insert, Delete.
    pub fn parse(text: &str) -> Option<Self> {
        let mut mods = 0;
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            if let Some(m) = modifier(part) {
                mods |= m;
            } else if key.is_none() {
                key = Some(parse_key(part)?);
            } else {
                return None;
            }
        }
        (mods != 0).then_some(Self { mods, vk: key? })
    }

    /// None for no modifiers, unknown modifier bits or an unsupported key.
    pub fn try_create(mods: u8, vk: u16) -> Option<Self> {
        (mods != 0 && mods & !Self::ALL_MODS == 0 && is_supported_key(vk)).then_some(Self { mods, vk })
    }
}

/// Canonical form: Ctrl, Alt, Shift, Win, then the key.
impl fmt::Display for Gesture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (bit, name) in [(Self::MOD_CONTROL, "Ctrl"), (Self::MOD_ALT, "Alt"), (Self::MOD_SHIFT, "Shift"), (Self::MOD_WIN, "Win")] {
            if self.mods & bit != 0 {
                write!(f, "{name}+")?;
            }
        }
        match self.vk {
            0x30..=0x39 | 0x41..=0x5A => write!(f, "{}", self.vk as u8 as char),
            0x70..=0x87 => write!(f, "F{}", self.vk - 0x6F),
            vk => match NAMED_KEYS.iter().find(|(_, v)| *v == vk) {
                Some((name, _)) => f.write_str(name),
                None => write!(f, "0x{vk:02X}"),
            },
        }
    }
}

// The first name per key is the canonical one.
const NAMED_KEYS: [(&str, u16); 12] = [
    ("PageUp", 0x21), ("PgUp", 0x21), ("PageDown", 0x22), ("PgDn", 0x22), ("End", 0x23), ("Home", 0x24),
    ("Left", 0x25), ("Up", 0x26), ("Right", 0x27), ("Down", 0x28), ("Insert", 0x2D), ("Delete", 0x2E),
];

fn modifier(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Gesture::MOD_CONTROL),
        "alt" => Some(Gesture::MOD_ALT),
        "shift" => Some(Gesture::MOD_SHIFT),
        "win" => Some(Gesture::MOD_WIN),
        _ => None,
    }
}

fn parse_key(name: &str) -> Option<u16> {
    if let Some((_, vk)) = NAMED_KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        return Some(*vk);
    }
    let bytes = name.as_bytes();
    if let [c] = bytes {
        return c.is_ascii_alphanumeric().then(|| u16::from(c.to_ascii_uppercase()));
    }
    let digits = name.strip_prefix(['F', 'f'])?;
    if !(1..=2).contains(&digits.len()) || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u16 = digits.parse().ok()?;
    (1..=24).contains(&n).then_some(0x6F + n)
}

fn is_supported_key(vk: u16) -> bool {
    matches!(vk, 0x30..=0x39 | 0x41..=0x5A | 0x70..=0x87) || NAMED_KEYS.iter().any(|(_, v)| *v == vk)
}

/// A shortcut action. Declaration order = settings-card order; Win32 hotkey id = [`Action::id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    BrightnessUp,
    BrightnessDown,
    ContrastUp,
    ContrastDown,
    DayPreset,
    NightPreset,
    InputHdmi,
    InputDp,
    InputUsbC,
    SwitchToMe,
    OpenPanel,
}

impl Action {
    pub const ALL: [Action; 11] = [
        Action::BrightnessUp, Action::BrightnessDown, Action::ContrastUp, Action::ContrastDown, Action::DayPreset,
        Action::NightPreset, Action::InputHdmi, Action::InputDp, Action::InputUsbC, Action::SwitchToMe, Action::OpenPanel,
    ];

    /// The `settings.json` key (the 0.0.4 enum name).
    pub fn key(self) -> &'static str {
        self.info().0
    }

    pub fn label(self) -> &'static str {
        self.info().1
    }

    /// The default gesture text.
    pub fn default_gesture(self) -> &'static str {
        self.info().2
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Win32 hotkey id (`action + 1`), as 0.0.4 registered it.
    pub fn id(self) -> i32 {
        self as i32 + 1
    }

    fn info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Action::BrightnessUp => ("BrightnessUp", "Brightness up", "Ctrl+Alt+Up"),
            Action::BrightnessDown => ("BrightnessDown", "Brightness down", "Ctrl+Alt+Down"),
            Action::ContrastUp => ("ContrastUp", "Contrast up", "Ctrl+Alt+Left"),
            Action::ContrastDown => ("ContrastDown", "Contrast down", "Ctrl+Alt+Right"),
            Action::DayPreset => ("DayPreset", "Day preset", "Ctrl+Alt+Home"),
            Action::NightPreset => ("NightPreset", "Night preset", "Ctrl+Alt+End"),
            Action::InputHdmi => ("InputHdmi", "Input HDMI", "Ctrl+Alt+F1"),
            Action::InputDp => ("InputDp", "Input DisplayPort", "Ctrl+Alt+F2"),
            Action::InputUsbC => ("InputUsbC", "Input USB-C", "Ctrl+Alt+F3"),
            Action::SwitchToMe => ("SwitchToMe", "Switch to this PC", "Ctrl+Alt+M"),
            Action::OpenPanel => ("OpenPanel", "Open ThinkVision Control", "Ctrl+Alt+O"),
        }
    }
}
