//! ThinkVision Control look-alike in wxWidgets (Rust + wxDragon). Static data, no monitor control.
//! Flags: --dark / --light (force theme), --monitor (start on the Monitor settings page),
//! --fluent (custom-drawn Fluent controls + system accent, see fluent.rs).
//! Icon font: FluentSystemIcons-Regular.ttf next to the exe, or the path in TVC_ICON_FONT.
#![windows_subsystem = "windows"]

mod fluent;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wxdragon::dc::PaintDC;
use wxdragon::widgets::simplebook::SimpleBook;
use wxdragon::prelude::*;
use wxdragon::{BrushStyle, DeviceContext, PenStyle};

#[derive(Clone, Copy)]
struct Theme {
    page: Colour,
    card: Colour,
    nav_sel: Colour,
    text: Colour,
    sub: Colour,
    accent: Colour,
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            page: Colour::rgb(0x20, 0x20, 0x20),
            card: Colour::rgb(0x2B, 0x2B, 0x2B),
            nav_sel: Colour::rgb(0x2D, 0x2D, 0x2D),
            text: Colour::rgb(0xFF, 0xFF, 0xFF),
            sub: Colour::rgb(0xC5, 0xC5, 0xC5),
            accent: Colour::rgb(0xF0, 0x5A, 0x5A),
        }
    } else {
        Theme {
            page: Colour::rgb(0xFB, 0xF1, 0xF3),
            card: Colour::rgb(0xFF, 0xFF, 0xFF),
            nav_sel: Colour::rgb(0xF1, 0xE3, 0xE6),
            text: Colour::rgb(0x1A, 0x1A, 0x1A),
            sub: Colour::rgb(0x5F, 0x5F, 0x5F),
            accent: Colour::rgb(0xC4, 0x14, 0x1E),
        }
    }
}

// Fluent System Icons (Regular) code points, from ViewModels/Glyphs.cs.
const G_DISPLAY: char = '\u{f35a}';
const G_OPTIONS: char = '\u{f588}';
const G_SETTINGS: char = '\u{f6aa}';
const G_BRIGHT: char = '\u{e1f8}';
const G_CONTRAST: char = '\u{f2be}';
const G_COLOUR: char = '\u{f2f6}';
const G_INPUT: char = '\u{e99a}'; // plug_connected (what Glyphs.Input uses today)
const G_INPUT_KM: char = '\u{f0d1e}'; // keyboard_mouse_20 (what the reference screenshot shows)
const G_CHECK: char = '\u{f295}';
const G_CHEVRON: char = '\u{f2a4}';
const G_OVERDRIVE: char = '\u{f831}';
const G_DYNCON: char = '\u{f33c}';
const G_ASPECT: char = '\u{e685}';
const G_USB: char = '\u{edd0}';
const G_LANG: char = '\u{f4f4}';
const G_POWER: char = '\u{f60f}';

const ICON_FACE: &str = "FluentSystemIcons-Regular";

#[cfg(windows)]
const UI_FACE: &str = "Segoe UI";
#[cfg(not(windows))]
const UI_FACE: &str = "";

fn font(pt: i32, weight: FontWeight) -> Font {
    let mut b = Font::builder().with_point_size(pt).with_weight(weight);
    if !UI_FACE.is_empty() {
        b = b.with_face_name(UI_FACE);
    }
    b.build().expect("font")
}

fn icon_font(pt: i32) -> Font {
    Font::builder().with_point_size(pt).with_face_name(ICON_FACE).build().expect("icon font")
}

struct Ctx {
    t: Theme,
    body: Font,
    small: Font,
    section: Font,
    fluent: bool,
    pal: fluent::Pal,
    acc: fluent::Acc,
}

fn label<P: WxWidget>(parent: &P, text: &str, f: &Font, fg: Colour, bg: Colour) -> StaticText {
    let s = StaticText::builder(parent).with_label(text).build();
    s.set_font(f);
    s.set_foreground_color(fg);
    s.set_background_color(bg);
    s
}

fn glyph<P: WxWidget>(parent: &P, g: char, pt: i32, fg: Colour, bg: Colour) -> StaticText {
    label(parent, &g.to_string(), &icon_font(pt), fg, bg)
}

/// A "card": a panel whose background is the card colour; the corners are painted in the page
/// colour around a GDI rounded rectangle (not anti-aliased). Native child controls take the panel colour.
fn card<P: WxWidget>(parent: &P, c: &Ctx, g: char, title: &str, subtitle: Option<&str>) -> (Panel, BoxSizer) {
    let t = c.t;
    let p = Panel::builder(parent).build();
    p.set_background_color(t.card);
    let pp = p.clone();
    if c.fluent {
        fluent::card_bg(&p, t.page, t.card, c.pal.card_stroke);
    } else { p.on_paint(move |_| {
        let dc = PaintDC::new(&pp);
        let s = pp.get_size();
        dc.set_pen(t.page, 1, PenStyle::Solid);
        dc.set_brush(t.page, BrushStyle::Solid);
        dc.draw_rectangle(0, 0, s.width, s.height);
        dc.set_pen(t.card, 1, PenStyle::Solid);
        dc.set_brush(t.card, BrushStyle::Solid);
        dc.draw_rounded_rectangle(0, 0, s.width, s.height, 5.0);
    }); }
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let ic = glyph(&p, g, 15, t.text, t.card);
    row.add(&ic, 0, SizerFlag::AlignCenterVertical | SizerFlag::Left | SizerFlag::Right, 16);
    let col = BoxSizer::builder(Orientation::Vertical).build();
    col.add(&label(&p, title, &c.body, t.text, t.card), 0, SizerFlag::AlignLeft, 0);
    if let Some(st) = subtitle {
        let s = label(&p, st, &c.small, t.sub, t.card);
        s.wrap(380);
        col.add(&s, 0, SizerFlag::AlignLeft | SizerFlag::Top, 1);
    }
    row.add_sizer(&col, 1, SizerFlag::AlignCenterVertical | SizerFlag::Top | SizerFlag::Bottom, 16);
    p.set_sizer(row.clone(), true);
    (p, row)
}

fn section<P: WxWidget>(parent: &P, page: &BoxSizer, c: &Ctx, text: &str, first: bool) {
    page.add_spacer(if first { 14 } else { 30 });
    page.add(&label(parent, text, &c.section, c.t.text, c.t.page), 0, SizerFlag::Bottom, 8);
}

fn add_card(page: &BoxSizer, p: &Panel) {
    page.add(p, 0, SizerFlag::Expand | SizerFlag::Bottom, 4);
}

fn choice(parent: &Panel, items: &[&str], sel: u32) -> Choice {
    Choice::builder(parent)
        .with_choices(items.iter().map(|s| s.to_string()).collect())
        .with_selection(Some(sel))
        .with_size(Size::new(140, -1))
        .build()
}

fn scroller(book: &SimpleBook, c: &Ctx) -> (ScrolledWindow, BoxSizer) {
    let sw = ScrolledWindow::builder(book).build();
    sw.set_background_color(c.t.page);
    sw.set_scroll_rate(0, 10);
    (sw, BoxSizer::builder(Orientation::Vertical).build())
}

fn finish(sw: &ScrolledWindow, v: BoxSizer) {
    let pad = BoxSizer::builder(Orientation::Vertical).build();
    pad.add_sizer(&v, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 36);
    sw.set_sizer(pad, true);
}

fn display_page(book: &SimpleBook, c: &Ctx) -> ScrolledWindow {
    let t = c.t;
    let (sw, v) = scroller(book, c);
    v.add_spacer(22);
    v.add(&label(&sw, "Display", &font(21, FontWeight::SemiBold), t.text, t.page), 0, SizerFlag::Bottom, 14);

    // Header: monitor glyph, name, connection line.
    let hdr = BoxSizer::builder(Orientation::Horizontal).build();
    hdr.add(&glyph(&sw, G_DISPLAY, 36, t.text, t.page), 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 14);
    let hc = BoxSizer::builder(Orientation::Vertical).build();
    hc.add(&label(&sw, "ThinkVision P32p-20", &font(15, FontWeight::SemiBold), t.text, t.page), 0, SizerFlag::AlignLeft, 0);
    let st = BoxSizer::builder(Orientation::Horizontal).build();
    st.add(&glyph(&sw, G_CHECK, 10, t.sub, t.page), 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 6);
    st.add(&label(&sw, "Connected over HDMI \u{00B7} Firmware LB1.2", &c.body, t.sub, t.page), 0, SizerFlag::AlignCenterVertical, 0);
    hc.add_sizer(&st, 0, SizerFlag::AlignLeft, 0);
    hdr.add_sizer(&hc, 0, SizerFlag::AlignCenterVertical, 0);
    v.add_sizer(&hdr, 0, SizerFlag::Bottom, 4);

    section(&sw, &v, c, "Brightness and contrast", false);
    for (g, name, val) in [(G_BRIGHT, "Brightness", 0), (G_CONTRAST, "Contrast", 25)] {
        let (p, row) = card(&sw, c, g, name, None);
        let vl = label(&p, &val.to_string(), &c.body, t.text, t.card);
        vl.set_min_size(Size::new(28, -1));
        let vl2 = vl.clone();
        if c.fluent {
            let s = fluent::slider(&p, t.card, c.pal, c.acc.clone(), val, move |v| vl2.set_label(&v.to_string()));
            row.add(&s, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
        } else {
            let s = Slider::builder(&p)
                .with_value(val)
                .with_min_value(0)
                .with_max_value(100)
                .with_size(Size::new(210, -1))
                .build();
            row.add(&s, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
            s.on_slider(move |e| vl2.set_label(&e.get_value().to_string()));
        }
        row.add(&vl, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 16);
        p.set_min_size(Size::new(-1, 66));
        add_card(&v, &p);
    }

    section(&sw, &v, c, "Colour", false);
    let (p, row) = card(&sw, c, G_COLOUR, "Colour mode", Some("Changes apply straight away"));
    let ch = choice(&p, &["Neutral", "sRGB", "Reddish", "Bluish", "User / Custom"], 0);
    row.add(&ch, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 16);
    add_card(&v, &p);

    // Custom colour: card with a chevron that reveals R/G/B sliders (custom expander, not wxCollapsiblePane).
    let (p, row) = card(&sw, c, G_OPTIONS, "Custom colour", Some("Available in User / Custom colour mode"));
    let chev = glyph(&p, G_CHEVRON, 11, t.text, t.card);
    row.add(&chev, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 20);
    add_card(&v, &p);
    let body = Panel::builder(&sw).build();
    body.set_background_color(t.card);
    let bs = BoxSizer::builder(Orientation::Vertical).build();
    for (n, val) in [("Red", 50), ("Green", 50), ("Blue", 50)] {
        let r = BoxSizer::builder(Orientation::Horizontal).build();
        let l = label(&body, n, &c.body, t.text, t.card);
        l.set_min_size(Size::new(80, -1));
        r.add(&l, 0, SizerFlag::AlignCenterVertical | SizerFlag::Left, 62);
        r.add_stretch_spacer(1);
        if c.fluent {
            let s = fluent::slider(&body, t.card, c.pal, c.acc.clone(), val, |_| {});
            r.add(&s, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 52);
        } else {
            let s = Slider::builder(&body).with_value(val).with_size(Size::new(210, -1)).build();
            r.add(&s, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 52);
        }
        bs.add_sizer(&r, 0, SizerFlag::Expand | SizerFlag::Top | SizerFlag::Bottom, 4);
    }
    body.set_sizer(bs, true);
    body.show(false);
    v.add(&body, 0, SizerFlag::Expand | SizerFlag::Bottom, 4);
    let tg = {
        let (b2, sw2, chev2) = (body.clone(), sw.clone(), chev.clone());
        Rc::new(move || {
            let open = !b2.is_shown();
            b2.show(open);
            chev2.set_label(&(if open { '\u{f2b7}' } else { G_CHEVRON }).to_string());
            sw2.layout();
        })
    };
    {
        let tg2 = tg.clone();
        p.on_mouse_left_down(move |_| tg2());
        chev.on_mouse_left_down(move |_| tg());
    }

    section(&sw, &v, c, "Input", false);
    let (p, row) = card(&sw, c, if c.fluent { G_INPUT_KM } else { G_INPUT }, "Input", Some("This PC uses HDMI"));
    if c.fluent {
        fluent::radios(&p, &row, t.card, c.pal, c.acc.clone(), &c.body, &["HDMI", "DisplayPort", "USB-C"], 0);
    } else { for (i, n) in ["HDMI", "DisplayPort", "USB-C"].iter().enumerate() {
        let mut b = RadioButton::builder(&p).with_label(n);
        if i == 0 {
            b = b.with_style(RadioButtonStyle::GroupStart);
        }
        let rb = b.build();
        rb.set_font(&c.body);
        rb.set_foreground_color(t.text);
        if i == 0 {
            rb.set_value(true);
        }
        row.add(&rb, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 28);
    } }
    add_card(&v, &p);

    finish(&sw, v);
    sw
}

fn toggle_row(p: &Panel, row: &BoxSizer, c: &Ctx) {
    let off = label(p, "Off", &c.body, c.t.text, c.t.card);
    row.add(&off, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
    let o2 = off.clone();
    if c.fluent {
        let sw = fluent::toggle(p, c.t.card, c.pal, c.acc.clone(), move |on| o2.set_label(if on { "On" } else { "Off" }));
        row.add(&sw, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 16);
        return;
    }
    // wxWidgets 3.3.4 has no toggle-switch control; the nearest native control is a check box.
    let cb = CheckBox::builder(p).build();
    row.add(&cb, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 16);
    cb.on_toggled(move |e| o2.set_label(if e.is_checked() { "On" } else { "Off" }));
}

fn monitor_page(book: &SimpleBook, c: &Ctx) -> ScrolledWindow {
    let t = c.t;
    let (sw, v) = scroller(book, c);
    v.add_spacer(22);
    v.add(&label(&sw, "Monitor settings", &font(21, FontWeight::SemiBold), t.text, t.page), 0, SizerFlag::AlignLeft, 0);

    section(&sw, &v, c, "Picture", true);
    let (p, row) = card(&sw, c, G_OVERDRIVE, "Over Drive", Some("Reduces motion blur. Extreme can leave trails behind moving objects"));
    row.add(&choice(&p, &["Off", "Normal", "Extreme"], 1), 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 16);
    add_card(&v, &p);
    let (p, row) = card(&sw, c, G_DYNCON, "Dynamic Contrast", Some("Adjusts contrast to match what's on screen"));
    toggle_row(&p, &row, c);
    add_card(&v, &p);
    let (p, row) = card(&sw, c, G_ASPECT, "Aspect ratio", Some("The monitor greys this out at its native resolution and may ignore changes"));
    row.add(&choice(&p, &["Full screen", "Original AR"], 0), 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 16);
    add_card(&v, &p);

    section(&sw, &v, c, "Ports", false);
    let (p, row) = card(&sw, c, G_USB, "USB Charging", Some("Charge devices from the monitor's USB ports while it's in standby"));
    toggle_row(&p, &row, c);
    add_card(&v, &p);

    section(&sw, &v, c, "On-screen menu", false);
    let (p, row) = card(&sw, c, G_LANG, "Menu language", Some("The language of the monitor's own menus"));
    row.add(
        &choice(&p, &["English", "Deutsch", "Fran\u{e7}ais", "\u{65e5}\u{672c}\u{8a9e}"], 0),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::Right,
        16,
    );
    add_card(&v, &p);

    section(&sw, &v, c, "Power", false);
    let (p, row) = card(&sw, c, G_POWER, "Power", Some("Put the monitor in standby or turn it off"));
    for n in ["Standby", "Power off"] {
        if c.fluent {
            let b = fluent::button(&p, t.card, c.pal, &c.body, n);
            row.add(&b, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
            continue;
        }
        let b = Button::builder(&p).with_label(n).build();
        row.add(&b, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 12);
    }
    row.add_spacer(4);
    add_card(&v, &p);

    finish(&sw, v);
    sw
}

struct NavItem {
    panel: Panel,
    bar: Panel,
    kids: Vec<StaticText>,
    sel: Rc<Cell<bool>>,
    hov: Rc<Cell<bool>>,
}

fn nav_item(parent: &Panel, c: &Ctx, g: char, text: &str) -> NavItem {
    let t = c.t;
    let p = Panel::builder(parent).build();
    p.set_background_color(t.page);
    p.set_min_size(Size::new(-1, 36));
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let bar = Panel::builder(&p).with_size(Size::new(3, 16)).build();
    bar.set_background_color(t.page);
    row.add(&bar, 0, SizerFlag::AlignCenterVertical, 0);
    let ic = glyph(&p, g, 13, t.text, t.page);
    row.add(&ic, 0, SizerFlag::AlignCenterVertical | SizerFlag::Left | SizerFlag::Right, 12);
    let l = label(&p, text, &c.body, t.text, t.page);
    row.add(&l, 1, SizerFlag::AlignCenterVertical | SizerFlag::Left, 4);
    p.set_sizer(row, true);
    let (sel, hov) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let kids = vec![ic, l];
    if c.fluent {
        // Rounded selection pill, accent indicator and hover fill, painted anti-aliased.
        bar.show(false);
        fluent::nav_bg(&p, t.page, c.pal, c.acc.clone(), sel.clone(), hov.clone());
        let mk = |enter: bool| {
            let (p2, kids2, sel2, hov2, pal) = (p.clone(), kids.clone(), sel.clone(), hov.clone(), c.pal);
            move |_e: WindowEventData| {
                hov2.set(enter);
                let bg = fluent::nav_fill(pal, t.page, sel2.get(), enter);
                for k in &kids2 {
                    k.set_background_color(bg);
                    k.refresh(true, None);
                }
                p2.refresh(false, None);
            }
        };
        p.on_mouse_enter(mk(true));
        p.on_mouse_leave(mk(false));
        for k in &kids {
            k.on_mouse_enter(mk(true));
            k.on_mouse_leave(mk(false));
        }
    }
    NavItem { panel: p, bar, kids, sel, hov }
}

/// wxDragon builds wxGTK with CMake, whose default is wxUSE_PRIVATE_FONTS=OFF on GTK, so
/// wxFont::AddPrivateFont is compiled out. Workaround for the look-alike: put the TTF in the
/// user's fontconfig directory before GTK starts. (Real fix: build wx with -DwxUSE_PRIVATE_FONTS=ON.)
#[cfg(not(windows))]
fn install_icon_font_for_gtk() {
    let Ok(home) = std::env::var("HOME") else { return };
    let dir = std::path::Path::new(&home).join(".local/share/fonts");
    let dst = dir.join("FluentSystemIcons-Regular.ttf");
    if dst.exists() {
        return;
    }
    let src = std::env::var("TVC_ICON_FONT").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::env::current_exe().unwrap_or_default().with_file_name("FluentSystemIcons-Regular.ttf")
    });
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::copy(src, dst);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let force: Option<bool> = if has("--dark") { Some(true) } else if has("--light") { Some(false) } else { None };
    let start_monitor = has("--monitor");
    let fluent_mode = has("--fluent");

    #[cfg(not(windows))]
    install_icon_font_for_gtk();

    let _ = wxdragon::main(move |_| {
        let dark = match force {
            Some(d) => {
                let _ = wxdragon::app::set_appearance(if d { Appearance::Dark } else { Appearance::Light });
                d
            }
            None => {
                let _ = wxdragon::app::set_appearance(Appearance::System);
                wxdragon::appearance::is_system_dark_mode()
            }
        };
        // GTK: the theme comes from GTK_THEME; let our own colours follow it when no flag was given.
        let dark = if cfg!(windows) || force.is_some() {
            dark
        } else {
            std::env::var("GTK_THEME").map(|v| v.ends_with(":dark")).unwrap_or(dark)
        };

        let font_path = std::env::var("TVC_ICON_FONT").unwrap_or_else(|_| {
            let exe = std::env::current_exe().unwrap_or_default();
            exe.with_file_name("FluentSystemIcons-Regular.ttf").to_string_lossy().into_owned()
        });
        let _ = Font::add_private_font(&font_path);

        let pal = fluent::pal(dark);
        let acc: fluent::Acc = Rc::new(Cell::new(fluent::read_accent(dark, theme(dark).card)));
        let c = Ctx {
            t: theme(dark),
            body: font(10, FontWeight::Normal),
            small: font(9, FontWeight::Normal),
            section: font(10, FontWeight::SemiBold),
            fluent: fluent_mode,
            pal,
            acc: acc.clone(),
        };
        let t = c.t;

        // 1022x727 outer = 1008x720 visible on Windows 11 (wx sizes include the 7 px invisible resize borders).
        let frame = Frame::builder().with_title("ThinkVision Control").with_size(Size::new(1022, 727)).build();
        let root = Panel::builder(&frame).build();
        root.set_background_color(t.page);
        let hs = BoxSizer::builder(Orientation::Horizontal).build();

        // Navigation pane.
        let nav = Panel::builder(&root).build();
        nav.set_background_color(t.page);
        nav.set_min_size(Size::new(320, -1));
        let ns = BoxSizer::builder(Orientation::Vertical).build();
        let ham = label(&nav, "\u{2630}", &font(12, FontWeight::Normal), t.text, t.page);
        ns.add(&ham, 0, SizerFlag::Left | SizerFlag::Top | SizerFlag::Bottom, 14);
        let items = Rc::new(RefCell::new(vec![
            nav_item(&nav, &c, G_DISPLAY, "Display"),
            nav_item(&nav, &c, G_OPTIONS, "Monitor settings"),
        ]));
        for it in items.borrow().iter() {
            ns.add(&it.panel, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 6);
        }
        ns.add_stretch_spacer(1);
        let line = StaticLine::builder(&nav).build();
        ns.add(&line, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
        let app_settings = nav_item(&nav, &c, G_SETTINGS, "App settings");
        ns.add(&app_settings.panel, 0, SizerFlag::Expand | SizerFlag::All, 6);
        nav.set_sizer(ns, true);
        hs.add(&nav, 0, SizerFlag::Expand, 0);

        // Pages.
        let book = SimpleBook::builder(&root).build();
        book.set_background_color(t.page);
        let p0 = display_page(&book, &c);
        let p1 = monitor_page(&book, &c);
        book.add_page(&p0, "Display", true, None);
        book.add_page(&p1, "Monitor settings", false, None);
        hs.add(&book, 1, SizerFlag::Expand, 0);
        root.set_sizer(hs, true);

        let select = {
            let items = items.clone();
            let book = book.clone();
            Rc::new(move |idx: usize| {
                for (i, it) in items.borrow().iter().enumerate() {
                    it.sel.set(i == idx);
                    let bg = if fluent_mode {
                        fluent::nav_fill(pal, t.page, i == idx, it.hov.get())
                    } else if i == idx {
                        t.nav_sel
                    } else {
                        t.page
                    };
                    it.panel.set_background_color(bg);
                    for k in &it.kids {
                        k.set_background_color(bg);
                    }
                    it.bar.set_background_color(if i == idx { t.accent } else { bg });
                    it.panel.refresh(true, None);
                    for k in &it.kids {
                        k.refresh(true, None);
                    }
                }
                book.set_selection(idx);
            })
        };
        for (i, it) in items.borrow().iter().enumerate() {
            let s = select.clone();
            it.panel.on_mouse_left_down(move |_| s(i));
            let s = select.clone();
            it.bar.on_mouse_left_down(move |_| s(i));
            for k in &it.kids {
                let s = select.clone();
                k.on_mouse_left_down(move |_| s(i));
            }
        }
        select(if start_monitor { 1 } else { 0 });

        if fluent_mode {
            // Accent changes: re-read the palette and repaint (wx forwards WM_SYSCOLORCHANGE as this event).
            let (acc2, f2) = (acc.clone(), frame.clone());
            frame.bind_internal(EventType::SYS_COLOUR_CHANGED, move |e| {
                acc2.set(fluent::read_accent(dark, t.card));
                f2.refresh(true, None);
                e.skip(true);
            });
            frame.on_char_hook(|e| {
                let code = match &e {
                    WindowEventData::Keyboard(k) => k.get_key_code(),
                    WindowEventData::General(ev) => ev.get_key_code(),
                    _ => None,
                };
                if matches!(code, Some(WXK_TAB) | Some(WXK_LEFT) | Some(WXK_RIGHT) | Some(WXK_UP) | Some(WXK_DOWN)) {
                    fluent::set_keyboard_mode(true);
                }
                e.skip(true);
            });
            if std::env::var("TVC_PRINT_ACCENT").is_ok() {
                let a = acc.get();
                eprintln!("accent #{:02X}{:02X}{:02X} from {}", a.fill.r, a.fill.g, a.fill.b, a.source);
            }
        }
        frame.centre();
        frame.show(true);
    });
}
