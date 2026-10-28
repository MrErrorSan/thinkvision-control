// iced probe window: label, slider 0-100, toggler, pick_list (3 items), button.
use iced::widget::{button, column, pick_list, slider, text, toggler};
use iced::{Element, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Standard,
    Movie,
    Game,
}

impl Mode {
    const ALL: [Mode; 3] = [Mode::Standard, Mode::Movie, Mode::Game];
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Mode::Standard => "Standard",
            Mode::Movie => "Movie",
            Mode::Game => "Game",
        })
    }
}

#[derive(Debug, Clone)]
enum Message {
    Slide(u8),
    Toggle(bool),
    Pick(Mode),
    Apply,
}

struct Probe {
    status: String,
    value: u8,
    enabled: bool,
    mode: Option<Mode>,
}

impl Probe {
    fn new() -> Self {
        Probe { status: "Brightness".into(), value: 50, enabled: false, mode: Some(Mode::Standard) }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Slide(v) => self.value = v,
            Message::Toggle(b) => self.enabled = b,
            Message::Pick(m) => self.mode = Some(m),
            Message::Apply => self.status = "clicked".into(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        column![
            text(self.status.as_str()),
            slider(0..=100, self.value, Message::Slide),
            toggler(self.enabled).label("Enabled").on_toggle(Message::Toggle),
            pick_list(&Mode::ALL[..], self.mode, Message::Pick),
            button("Apply").on_press(Message::Apply),
        ]
        .spacing(12)
        .padding(16)
        .into()
    }
}

fn main() -> iced::Result {
    iced::application(Probe::new, Probe::update, Probe::view)
        .title("probe")
        .window_size(Size::new(600.0, 400.0))
        .run()
}
