//! `--fluent` variant: custom-drawn, anti-aliased Fluent-style controls on plain wxPanels.
//!
//! wxDragon 0.9.24 does not expose wxGraphicsContext, so shapes are rasterised here with a tiny
//! signed-distance-field painter into an RGBA buffer, turned into a wxBitmap and blitted with
//! wxPaintDC::DrawBitmap. Text stays native (wxStaticText or DC text, ClearType on Windows).

use std::cell::Cell;
use std::rc::Rc;
use wxdragon::dc::{ClientDC, PaintDC};
use wxdragon::prelude::*;
use wxdragon::DeviceContext;

// ---------------------------------------------------------------- accent

#[derive(Clone, Copy)]
pub struct Accent {
    /// AccentFillColorDefault: AccentLight2 in dark mode, AccentDark1 in light mode.
    pub fill: Colour,
    /// Hover variant (AccentLight2/Dark1 at 90 %), TextOnAccent for glyphs drawn on the fill.
    pub hover: Colour,
    pub on_fill: Colour,
    pub source: &'static str,
}

pub type Acc = Rc<Cell<Accent>>;

fn mix(a: Colour, b: Colour, t: f32) -> Colour {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Colour::rgb(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b))
}

pub fn read_accent(dark: bool, bg: Colour) -> Accent {
    let (fill, source) = platform_accent(dark).unwrap_or((
        if dark { Colour::rgb(0x60, 0xCD, 0xFF) } else { Colour::rgb(0x00, 0x5F, 0xB8) },
        "default (#0078D4 palette)",
    ));
    Accent {
        fill,
        hover: mix(fill, bg, 0.1),
        on_fill: if dark { Colour::rgb(0, 0, 0) } else { Colour::rgb(0xFF, 0xFF, 0xFF) },
        source,
    }
}

#[cfg(windows)]
fn platform_accent(dark: bool) -> Option<(Colour, &'static str)> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(hkey: isize, sub: *const u16, val: *const u16, flags: u32, ty: *mut u32, data: *mut u8, len: *mut u32) -> i32;
    }
    const HKCU: isize = 0x8000_0001u32 as i32 as isize;
    const RRF_RT_REG_BINARY: u32 = 0x08;
    const RRF_RT_REG_DWORD: u32 = 0x10;
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    // AccentPalette: 8 x RGBA = Light3, Light2, Light1, Accent, Dark1, Dark2, Dark3, (unused).
    let mut buf = [0u8; 32];
    let mut len = 32u32;
    let rc = unsafe {
        RegGetValueW(HKCU, w("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent").as_ptr(),
            w("AccentPalette").as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), buf.as_mut_ptr(), &mut len)
    };
    if rc == 0 && len >= 32 {
        let i = if dark { 1 } else { 4 };
        return Some((Colour::rgb(buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2]), "HKCU Explorer\\Accent\\AccentPalette"));
    }
    // Fallback: DWM AccentColor (0xAABBGGRR), shaded towards white/black like Light2/Dark1.
    let mut v = 0u32;
    let mut len = 4u32;
    let rc = unsafe {
        RegGetValueW(HKCU, w("Software\\Microsoft\\Windows\\DWM").as_ptr(), w("AccentColor").as_ptr(),
            RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut v as *mut u32 as *mut u8, &mut len)
    };
    if rc == 0 {
        let c = Colour::rgb(v as u8, (v >> 8) as u8, (v >> 16) as u8);
        let s = if dark { mix(c, Colour::rgb(255, 255, 255), 0.45) } else { mix(c, Colour::rgb(0, 0, 0), 0.15) };
        return Some((s, "HKCU DWM\\AccentColor"));
    }
    None
}

#[cfg(not(windows))]
fn platform_accent(_dark: bool) -> Option<(Colour, &'static str)> {
    // XDG desktop portal (GNOME 47+, KDE 6): org.freedesktop.appearance accent-color = (ddd) in 0..1.
    let out = std::process::Command::new("gdbus")
        .args(["call", "--session", "--timeout", "1", "--dest", "org.freedesktop.portal.Desktop",
            "--object-path", "/org/freedesktop/portal/desktop", "--method",
            "org.freedesktop.portal.Settings.ReadOne", "org.freedesktop.appearance", "accent-color"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let nums: Vec<f32> = s
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter_map(|t| t.parse().ok())
        .collect();
    if out.status.success() && nums.len() >= 3 && nums[..3].iter().all(|v| (0.0..=1.0).contains(v)) {
        let c = |v: f32| (v * 255.0).round() as u8;
        return Some((Colour::rgb(c(nums[0]), c(nums[1]), c(nums[2])), "XDG portal accent-color"));
    }
    // GTK3 has no accent concept; its theme selection colour is the closest thing.
    // Adwaita-dark's selection blue (#15539E) is too dark for a fill on a dark card, so lift it
    // the way Windows uses AccentLight2 in dark mode.
    let h = SystemSettings::get_colour(SystemColour::Highlight);
    let h = Colour::rgb(h.r, h.g, h.b);
    let h = if _dark { mix(h, Colour::rgb(255, 255, 255), 0.35) } else { h };
    Some((h, "GTK theme selection colour (no portal accent)"))
}

// ---------------------------------------------------------------- palette

#[derive(Clone, Copy)]
pub struct Pal {
    pub text: Colour,
    pub strong: Colour,       // ControlStrongStroke / ControlStrongFill
    pub alt: Colour,          // ControlAltFillSecondary
    pub thumb: Colour,        // ControlSolidFillDefault
    pub thumb_border: Colour, // ControlElevationBorder
    pub btn: Colour,
    pub btn_hover: Colour,
    pub btn_border: Colour,
    pub card_stroke: Colour,
    pub nav_hover: Colour,
    pub nav_sel: Colour,
}

pub fn pal(dark: bool) -> Pal {
    if dark {
        Pal {
            text: Colour::rgb(0xFF, 0xFF, 0xFF),
            strong: Colour::rgb(0x9E, 0x9E, 0x9E),
            alt: Colour::rgb(0x26, 0x26, 0x26),
            thumb: Colour::rgb(0x45, 0x45, 0x45),
            thumb_border: Colour::rgb(0x52, 0x52, 0x52),
            btn: Colour::rgb(0x37, 0x37, 0x37),
            btn_hover: Colour::rgb(0x3D, 0x3D, 0x3D),
            btn_border: Colour::rgb(0x43, 0x43, 0x43),
            card_stroke: Colour::rgb(0x1C, 0x1C, 0x1C),
            nav_hover: Colour::rgb(0x29, 0x29, 0x29),
            nav_sel: Colour::rgb(0x2D, 0x2D, 0x2D),
        }
    } else {
        Pal {
            text: Colour::rgb(0x1A, 0x1A, 0x1A),
            strong: Colour::rgb(0x8A, 0x8A, 0x8A),
            alt: Colour::rgb(0xF9, 0xF9, 0xF9),
            thumb: Colour::rgb(0xFF, 0xFF, 0xFF),
            thumb_border: Colour::rgb(0xE0, 0xE0, 0xE0),
            btn: Colour::rgb(0xFD, 0xFD, 0xFD),
            btn_hover: Colour::rgb(0xF6, 0xF6, 0xF6),
            btn_border: Colour::rgb(0xE2, 0xE2, 0xE2),
            card_stroke: Colour::rgb(0xEB, 0xE1, 0xE3),
            nav_hover: Colour::rgb(0xF5, 0xEA, 0xEC),
            nav_sel: Colour::rgb(0xF1, 0xE3, 0xE6),
        }
    }
}

// ---------------------------------------------------------------- SDF canvas

pub struct Canvas {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Canvas {
    pub fn new(w: i32, h: i32, bg: Colour) -> Self {
        let (w, h) = (w.max(1), h.max(1));
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            px.extend_from_slice(&[bg.r, bg.g, bg.b, 255]);
        }
        Canvas { w, h, px }
    }

    /// Rounded rectangle (x, y, w, h in px, may be fractional). `stroke` = Some(width) draws an
    /// inside outline instead of a fill. Coverage from the signed distance gives ~1 px anti-aliasing.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, stroke: Option<f32>, c: Colour) {
        let (hw, hh) = (w / 2.0, h / 2.0);
        let (cx, cy) = (x + hw, y + hh);
        let r = r.min(hw).min(hh);
        let x0 = ((x - 1.0).floor() as i32).max(0);
        let x1 = ((x + w + 1.0).ceil() as i32).min(self.w);
        let y0 = ((y - 1.0).floor() as i32).max(0);
        let y1 = ((y + h + 1.0).ceil() as i32).min(self.h);
        for py in y0..y1 {
            for px in x0..x1 {
                let qx = (px as f32 + 0.5 - cx).abs() - (hw - r);
                let qy = (py as f32 + 0.5 - cy).abs() - (hh - r);
                let d = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - r;
                let mut a = (0.5 - d).clamp(0.0, 1.0);
                if let Some(sw) = stroke {
                    a -= (0.5 - (d + sw)).clamp(0.0, 1.0);
                }
                if a <= 0.0 {
                    continue;
                }
                let i = ((py * self.w + px) * 4) as usize;
                for (k, s) in [c.r, c.g, c.b].into_iter().enumerate() {
                    let d0 = self.px[i + k] as f32;
                    self.px[i + k] = (d0 + (s as f32 - d0) * a).round() as u8;
                }
            }
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, stroke: Option<f32>, c: Colour) {
        self.rrect(cx - r, cy - r, 2.0 * r, 2.0 * r, r, stroke, c);
    }

    pub fn blit(&self, dc: &PaintDC) {
        if let Some(b) = Bitmap::from_rgba(&self.px, self.w as u32, self.h as u32) {
            dc.draw_bitmap(&b, 0, 0, false);
        }
    }
}

/// Make `p` paint itself entirely with `draw` (no erase, repaint on resize).
pub fn paint_with<F: Fn(&mut Canvas, i32, i32) + 'static>(p: &Panel, bg: impl Fn() -> Colour + 'static, draw: F) {
    p.set_background_style(BackgroundStyle::Paint);
    let pp = p.clone();
    p.on_paint(move |_| {
        let dc = PaintDC::new(&pp);
        let s = pp.get_size();
        let mut cv = Canvas::new(s.width, s.height, bg());
        draw(&mut cv, s.width, s.height);
        cv.blit(&dc);
    });
    let pp = p.clone();
    p.on_size(move |e| {
        pp.refresh(false, None);
        e.skip(true);
    });
}

fn pos(e: &WindowEventData) -> Option<Point> {
    match e {
        WindowEventData::MouseButton(m) => m.get_position(),
        WindowEventData::MouseMotion(m) => m.get_position(),
        _ => None,
    }
}

fn key(e: &WindowEventData) -> Option<i32> {
    let k = match e {
        WindowEventData::Keyboard(k) => k.get_key_code(),
        WindowEventData::General(ev) => ev.get_key_code(),
        _ => None,
    };
    if k.is_some() {
        set_keyboard_mode(true);
    }
    k
}

/// wxWANTS_CHARS controls receive Tab themselves, so hand it back to wx navigation.
fn tab(p: &Panel, e: &WindowEventData) -> bool {
    let (code, shift) = match e {
        WindowEventData::Keyboard(k) => (k.get_key_code(), k.shift_down()),
        WindowEventData::General(ev) => (ev.get_key_code(), ev.shift_down()),
        _ => (None, false),
    };
    if code == Some(WXK_TAB) {
        set_keyboard_mode(true);
        p.navigate(!shift);
        return true;
    }
    false
}

/// Hover + focus bookkeeping shared by every control.
fn track_hover_focus(p: &Panel, hover: &Rc<Cell<bool>>) {
    for (enter, h) in [(true, hover.clone()), (false, hover.clone())] {
        let pp = p.clone();
        let f = move |_e: WindowEventData| {
            h.set(enter);
            pp.refresh(false, None);
        };
        if enter { p.on_mouse_enter(f); } else { p.on_mouse_leave(f); }
    }
    let pp = p.clone();
    p.on_set_focus(move |e| { pp.refresh(false, None); e.skip(true); });
    let pp = p.clone();
    p.on_kill_focus(move |e| { pp.refresh(false, None); e.skip(true); });
}

thread_local! {
    /// Focus visuals only after keyboard navigation (like Windows/WinUI); a click hides them again.
    static KBD: Cell<bool> = const { Cell::new(false) };
}

pub fn set_keyboard_mode(on: bool) {
    KBD.with(|k| k.set(on));
}

fn focus_ring(cv: &mut Canvas, w: i32, h: i32, r: f32, c: Colour) {
    if !KBD.with(|k| k.get()) {
        return;
    }
    cv.rrect(0.0, 0.0, w as f32, h as f32, r, Some(2.0), c);
}

fn leaf(parent: &Panel, w: i32, h: i32) -> Panel {
    let p = Panel::builder(parent).with_size(Size::new(w, h)).build();
    p.set_min_size(Size::new(w, h));
    p.set_style_raw(p.get_style_raw() | 0x0004_0000); // wxWANTS_CHARS: arrow keys reach the control
    p
}

// ---------------------------------------------------------------- slider

pub fn slider(parent: &Panel, bg: Colour, pal: Pal, acc: Acc, value: i32, on_change: impl Fn(i32) + 'static) -> Panel {
    let p = leaf(parent, 214, 32);
    let v = Rc::new(Cell::new(value));
    let hover = Rc::new(Cell::new(false));
    let drag = Rc::new(Cell::new(false));
    const R: f32 = 10.0; // thumb radius; track runs between thumb centres
    {
        let (v, hover, drag, acc, pp) = (v.clone(), hover.clone(), drag.clone(), acc.clone(), p.clone());
        paint_with(&p, move || bg, move |cv, w, h| {
            let a = acc.get();
            let cy = h as f32 / 2.0;
            let x0 = R + 2.0;
            let x1 = w as f32 - R - 2.0;
            let tx = x0 + (x1 - x0) * v.get() as f32 / 100.0;
            cv.rrect(x0 - 2.0, cy - 2.0, x1 - x0 + 4.0, 4.0, 2.0, None, pal.strong);
            cv.rrect(x0 - 2.0, cy - 2.0, tx - x0 + 4.0, 4.0, 2.0, None, a.fill);
            cv.circle(tx, cy, R, None, pal.thumb_border);
            cv.circle(tx, cy, R - 1.0, None, pal.thumb);
            let inner = if drag.get() { 5.0 } else if hover.get() { 7.0 } else { 6.0 };
            cv.circle(tx, cy, inner, None, if hover.get() && !drag.get() { a.hover } else { a.fill });
            if pp.has_focus() {
                focus_ring(cv, w, h, 6.0, pal.text);
            }
        });
    }
    track_hover_focus(&p, &hover);
    let on_change = Rc::new(on_change);
    let set = {
        let (v, pp, oc) = (v.clone(), p.clone(), on_change.clone());
        Rc::new(move |nv: i32| {
            let nv = nv.clamp(0, 100);
            if nv != v.get() {
                v.set(nv);
                oc(nv);
            }
            pp.refresh(false, None);
        })
    };
    let from_x = {
        let pp = p.clone();
        move |x: i32| {
            let w = pp.get_size().width as f32;
            let (x0, x1) = (R + 2.0, w - R - 2.0);
            (((x as f32 - x0) / (x1 - x0)) * 100.0).round() as i32
        }
    };
    {
        let (pp, drag, set, fx) = (p.clone(), drag.clone(), set.clone(), from_x.clone());
        p.on_mouse_left_down(move |e| {
            set_keyboard_mode(false);
            pp.set_focus();
            pp.capture_mouse();
            drag.set(true);
            if let Some(pt) = pos(&e) { set(fx(pt.x)); }
        });
    }
    {
        let (drag, set, fx) = (drag.clone(), set.clone(), from_x.clone());
        p.on_mouse_motion(move |e| {
            if drag.get() {
                if let Some(pt) = pos(&e) { set(fx(pt.x)); }
            }
        });
    }
    {
        let (pp, drag) = (p.clone(), drag.clone());
        p.on_mouse_left_up(move |_| {
            if pp.has_capture() { pp.release_mouse(); }
            drag.set(false);
            pp.refresh(false, None);
        });
    }
    {
        let (v, set, pp) = (v.clone(), set.clone(), p.clone());
        p.on_key_down(move |e| if !tab(&pp, &e) { match key(&e) {
            Some(WXK_LEFT) | Some(WXK_DOWN) => set(v.get() - 1),
            Some(WXK_RIGHT) | Some(WXK_UP) => set(v.get() + 1),
            Some(WXK_PAGEUP) => set(v.get() + 10),
            Some(WXK_PAGEDOWN) => set(v.get() - 10),
            Some(WXK_HOME) => set(0),
            Some(WXK_END) => set(100),
            _ => e.skip(true),
        } });
    }
    p
}

// ---------------------------------------------------------------- toggle switch

pub fn toggle(parent: &Panel, bg: Colour, pal: Pal, acc: Acc, on_change: impl Fn(bool) + 'static) -> Panel {
    let p = leaf(parent, 44, 24);
    let on = Rc::new(Cell::new(false));
    let hover = Rc::new(Cell::new(false));
    {
        let (on, hover, acc, pp) = (on.clone(), hover.clone(), acc.clone(), p.clone());
        paint_with(&p, move || bg, move |cv, w, h| {
            let a = acc.get();
            let (x, y, tw, th) = (2.0, (h as f32 - 20.0) / 2.0, 40.0, 20.0);
            let kr = if hover.get() { 7.0 } else { 6.0 };
            if on.get() {
                cv.rrect(x, y, tw, th, 10.0, None, if hover.get() { a.hover } else { a.fill });
                cv.circle(x + tw - 10.0, y + 10.0, kr, None, a.on_fill);
            } else {
                cv.rrect(x, y, tw, th, 10.0, None, pal.alt);
                cv.rrect(x, y, tw, th, 10.0, Some(1.0), pal.strong);
                cv.circle(x + 10.0, y + 10.0, kr, None, pal.strong);
            }
            if pp.has_focus() {
                focus_ring(cv, w, h, 12.0, pal.text);
            }
        });
    }
    track_hover_focus(&p, &hover);
    let flip = {
        let (on, pp) = (on.clone(), p.clone());
        Rc::new(move || {
            on.set(!on.get());
            on_change(on.get());
            pp.refresh(false, None);
        })
    };
    {
        let (pp, f) = (p.clone(), flip.clone());
        p.on_mouse_left_down(move |_| { set_keyboard_mode(false); pp.set_focus(); f(); });
    }
    let pp = p.clone();
    p.on_key_down(move |e| if !tab(&pp, &e) { match key(&e) {
        Some(WXK_SPACE) | Some(WXK_RETURN) => flip(),
        _ => e.skip(true),
    } });
    p
}

// ---------------------------------------------------------------- radio group

pub fn radios(parent: &Panel, row: &BoxSizer, bg: Colour, pal: Pal, acc: Acc, font: &Font, labels: &[&str], sel: usize) {
    let cur = Rc::new(Cell::new(sel));
    let mut circles: Vec<Panel> = Vec::new();
    let all: Rc<Cell<Option<Rc<Vec<Panel>>>>> = Rc::new(Cell::new(None));
    for (i, l) in labels.iter().enumerate() {
        let c = leaf(parent, 24, 24);
        let hover = Rc::new(Cell::new(false));
        {
            let (cur, hover, acc, pp) = (cur.clone(), hover.clone(), acc.clone(), c.clone());
            paint_with(&c, move || bg, move |cv, w, h| {
                let a = acc.get();
                let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
                if cur.get() == i {
                    cv.circle(cx, cy, 10.0, None, if hover.get() { a.hover } else { a.fill });
                    cv.circle(cx, cy, if hover.get() { 5.0 } else { 4.0 }, None, a.on_fill);
                } else {
                    cv.circle(cx, cy, 10.0, None, if hover.get() { pal.btn_hover } else { pal.alt });
                    cv.circle(cx, cy, 10.0, Some(1.0), pal.strong);
                }
                if pp.has_focus() {
                    focus_ring(cv, w, h, 12.0, pal.text);
                }
            });
        }
        track_hover_focus(&c, &hover);
        let lab = StaticText::builder(parent).with_label(l).build();
        lab.set_font(font);
        lab.set_foreground_color(pal.text);
        lab.set_background_color(bg);
        row.add(&c, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
        row.add(&lab, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 28);
        let pick = {
            let (cur, all, c2) = (cur.clone(), all.clone(), c.clone());
            move |j: usize| {
                cur.set(j);
                if let Some(v) = all.take() {
                    for p in v.iter() { p.refresh(false, None); }
                    if let Some(t) = v.get(j) { t.set_focus(); }
                    all.set(Some(v));
                } else {
                    c2.refresh(false, None);
                }
            }
        };
        let pk = pick.clone();
        c.on_mouse_left_down(move |_| { set_keyboard_mode(false); pk(i) });
        let pk = pick.clone();
        lab.on_mouse_left_down(move |_| { set_keyboard_mode(false); pk(i) });
        let n = labels.len();
        let (cur2, cc) = (cur.clone(), c.clone());
        c.on_key_down(move |e| if !tab(&cc, &e) { match key(&e) {
            Some(WXK_LEFT) | Some(WXK_UP) => pick((cur2.get() + n - 1) % n),
            Some(WXK_RIGHT) | Some(WXK_DOWN) => pick((cur2.get() + 1) % n),
            Some(WXK_SPACE) => pick(i),
            _ => e.skip(true),
        } });
        circles.push(c);
    }
    all.set(Some(Rc::new(circles)));
}

// ---------------------------------------------------------------- button

pub fn button(parent: &Panel, bg: Colour, pal: Pal, font: &Font, label: &str) -> Panel {
    let tmp = ClientDC::new(parent);
    tmp.set_font(font);
    let (tw, th) = tmp.get_text_extent(label);
    drop(tmp);
    let (w, h) = (tw + 24 + 2, 32);
    let p = leaf(parent, w.max(96), h);
    let hover = Rc::new(Cell::new(false));
    let down = Rc::new(Cell::new(false));
    {
        let (hover, down, pp, font, label) = (hover.clone(), down.clone(), p.clone(), font.clone(), label.to_string());
        p.set_background_style(BackgroundStyle::Paint);
        p.on_paint(move |_| {
            let dc = PaintDC::new(&pp);
            let s = pp.get_size();
            let mut cv = Canvas::new(s.width, s.height, bg);
            let f = if down.get() { pal.alt } else if hover.get() { pal.btn_hover } else { pal.btn };
            cv.rrect(1.0, 1.0, s.width as f32 - 2.0, s.height as f32 - 2.0, 4.0, None, pal.btn_border);
            cv.rrect(2.0, 2.0, s.width as f32 - 4.0, s.height as f32 - 4.0, 3.0, None, f);
            if pp.has_focus() {
                focus_ring(&mut cv, s.width, s.height, 5.0, pal.text);
            }
            cv.blit(&dc);
            dc.set_font(&font);
            dc.set_text_foreground(pal.text);
            dc.set_background_mode(wxdragon::dc::BackgroundMode::Transparent);
            dc.draw_text(&label, (s.width - tw) / 2, (s.height - th) / 2);
        });
    }
    track_hover_focus(&p, &hover);
    {
        let (pp, down) = (p.clone(), down.clone());
        p.on_mouse_left_down(move |_| { set_keyboard_mode(false); down.set(true); pp.set_focus(); pp.refresh(false, None); });
    }
    {
        let (pp, down) = (p.clone(), down.clone());
        p.on_mouse_left_up(move |_| { down.set(false); pp.refresh(false, None); });
    }
    let pp = p.clone();
    p.on_key_down(move |e| if !tab(&pp, &e) { e.skip(true) });
    p
}

// ---------------------------------------------------------------- card + nav backgrounds

pub fn card_bg(p: &Panel, page: Colour, card: Colour, stroke: Colour) {
    paint_with(p, move || page, move |cv, w, h| {
        cv.rrect(0.0, 0.0, w as f32, h as f32, 5.0, None, stroke);
        cv.rrect(1.0, 1.0, w as f32 - 2.0, h as f32 - 2.0, 4.0, None, card);
    });
}

/// Nav item: rounded selection pill + accent indicator, hover fill. `state` = (selected, hovered).
pub fn nav_bg(p: &Panel, page: Colour, pal: Pal, acc: Acc, sel: Rc<Cell<bool>>, hov: Rc<Cell<bool>>) {
    paint_with(p, move || page, move |cv, w, h| {
        let (s, ho) = (sel.get(), hov.get());
        if s || ho {
            cv.rrect(0.0, 0.0, w as f32, h as f32, 4.0, None, nav_fill(pal, page, s, ho));
        }
        if s {
            cv.rrect(0.0, h as f32 / 2.0 - 8.0, 3.0, 16.0, 1.5, None, acc.get().fill);
        }
    });
}

pub fn nav_fill(pal: Pal, page: Colour, sel: bool, hov: bool) -> Colour {
    if sel && hov { mix(pal.nav_sel, pal.text, 0.03) } else if sel { pal.nav_sel } else if hov { pal.nav_hover } else { page }
}
