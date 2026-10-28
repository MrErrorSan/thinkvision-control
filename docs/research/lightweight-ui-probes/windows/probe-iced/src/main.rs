#![windows_subsystem = "windows"]
use iced::widget::{button, column, pick_list, slider, text, toggler};
use iced::Element;

const OPTS: [&str; 3] = ["sRGB", "DCI-P3", "Custom"];

struct State { v: u8, on: bool, sel: Option<&'static str> }
impl Default for State { fn default() -> Self { Self { v: 50, on: false, sel: Some(OPTS[0]) } } }

#[derive(Debug, Clone)]
enum Msg { Slide(u8), Toggle(bool), Pick(&'static str), Press }

fn update(s: &mut State, m: Msg) {
    match m { Msg::Slide(v) => s.v = v, Msg::Toggle(b) => s.on = b, Msg::Pick(p) => s.sel = Some(p), Msg::Press => {} }
}

fn view(s: &State) -> Element<'_, Msg> {
    column![
        text("Brightness"),
        slider(0..=100, s.v, Msg::Slide),
        toggler(s.on).label("Auto").on_toggle(Msg::Toggle),
        pick_list(&OPTS[..], s.sel, Msg::Pick),
        button("Apply").on_press(Msg::Press),
    ]
    .spacing(12)
    .padding(24)
    .into()
}

fn main() -> iced::Result {
    iced::application(State::default, update, view)
        .title("probe")
        .window_size((600.0, 400.0))
        .run()
}
