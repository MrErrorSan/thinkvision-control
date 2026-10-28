// libcosmic probe window: label, slider 0-100, toggler, dropdown (3 items), button.
use cosmic::app::{Core, Settings, Task};
use cosmic::{executor, iced, widget, Application, ApplicationExt, Element};

const MODES: [&str; 3] = ["Standard", "Movie", "Game"];

#[derive(Debug, Clone)]
enum Msg {
    Slide(f32),
    Toggle(bool),
    Pick(usize),
    Apply,
}

struct Probe {
    core: Core,
    status: String,
    value: f32,
    enabled: bool,
    mode: Option<usize>,
}

impl Application for Probe {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Msg;
    const APP_ID: &'static str = "dev.probe.cosmic";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Msg>) {
        let mut app = Probe { core, status: "Brightness".into(), value: 50.0, enabled: false, mode: Some(0) };
        let task = match app.core.main_window_id() {
            Some(id) => app.set_window_title("probe".into(), id),
            None => Task::none(),
        };
        (app, task)
    }

    fn update(&mut self, message: Msg) -> Task<Msg> {
        match message {
            Msg::Slide(v) => self.value = v,
            Msg::Toggle(b) => self.enabled = b,
            Msg::Pick(i) => self.mode = Some(i),
            Msg::Apply => self.status = "clicked".into(),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        widget::column(Vec::<Element<'_, Msg>>::new())
            .spacing(12)
            .padding(16)
            .push(widget::text(self.status.as_str()))
            .push(widget::slider(0.0..=100.0, self.value, Msg::Slide))
            .push(widget::toggler(self.enabled).label(String::from("Enabled")).on_toggle(Msg::Toggle))
            .push(widget::dropdown(&MODES, self.mode, Msg::Pick))
            .push(widget::button::standard("Apply").on_press(Msg::Apply))
            .into()
    }
}

fn main() -> iced::Result {
    let settings = Settings::default().size(iced::Size::new(600.0, 400.0));
    cosmic::app::run::<Probe>(settings, ())
}
