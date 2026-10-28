#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! ThinkVision Control. Command line: see `tvc_core::cli::help()` (`--help`). Developer flags: `--dark` /
//! `--light`, and in debug builds only `--demo` (a simulated monitor, data in the temp folder).
slint::include_modules!();

mod clipboard;
mod controller;
mod localtime;
mod osd;
mod platform;
mod tray;
mod view;

use slint::winit_030::winit::window::{Theme, WindowAttributes};
use tvc_core::cli::{self, CliCommand};
use tvc_core::engine::{Command, Engine};
use tvc_core::link::{MonitorLink, MonitorMatch};
use tvc_core::profile::ProfileStore;
use tvc_core::crash::Marker;
use tvc_core::{clock, log};

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let scheme = if has("--dark") { 1 } else if has("--light") { 2 } else { 0 };
    select_backend(scheme)?;

    let demo = cfg!(debug_assertions) && has("--demo");
    let data_dir = if demo { std::env::temp_dir().join("thinkvision-control-demo") } else { platform::data_dir() };
    let parsed = cli::parse(&args);
    if parsed.command != CliCommand::Show {
        platform::console::attach(); // so command-line output reaches the calling terminal
    }
    if parsed.command == CliCommand::Help {
        println!("{}", cli::help());
        return Ok(());
    }

    let _ = std::fs::create_dir_all(&data_dir);
    clock::set_utc_offset(localtime::utc_offset());
    log::init(&data_dir);

    // `--get` always reads on its own connection: a forwarded command can't return output.
    let get = matches!(parsed.command, CliCommand::Get(_));
    let listener = if demo || get {
        None
    } else {
        match platform::instance::start(&args) {
            platform::instance::Start::Forwarded => return Ok(()),
            platform::instance::Start::Primary(l) => Some(l),
        }
    };

    let profiles = ProfileStore::new(&data_dir);
    let profile = profiles.profile().clone();
    let link: Box<dyn MonitorLink> = if demo {
        Box::new(tvc_core::fake::FakeLink::new())
    } else {
        tvc_ddc::open(MonitorMatch { edid_id: profile.edid_id.clone(), description_hint: Some(profile.short_name.clone()) })
    };
    // a marker left by a crash during a risky monitor operation starts this run in safe mode.
    let marker = Marker::new(&data_dir);
    let safe_mode = marker.left_over().inspect(|op| log::write(&format!("Safe mode: crashed during {op}"))).is_some();
    marker.clear();
    let engine = Engine::spawn(
        link,
        profiles,
        data_dir.clone(),
        profile.timing.clone(),
        safe_mode,
        Box::new(|event| {
            let _ = slint::invoke_from_event_loop(move || controller::with(|c| c.on_event(event)));
        }),
    );
    let app = controller::Controller::new(engine, profile, data_dir, scheme, demo);
    controller::install(app.clone());

    if let Some(listener) = listener {
        listener.listen(|args| {
            let _ = slint::invoke_from_event_loop(move || controller::with(|c| c.on_forwarded(args)));
        });
    }

    app.send(Command::Connect);
    if parsed.command != CliCommand::Show && !demo {
        // A verb with no resident copy: connect once, run it, show the OSD, exit.
        app.start_headless(parsed.command, parsed.json);
    } else {
        // Also starts the retry burst and the display/power event watcher.
        app.start_resident(!(parsed.startup && app.background_mode()));
    }
    drop(app);

    slint::run_event_loop_until_quit()?;

    let controller = controller::uninstall();
    let exit_code = controller.as_ref().map_or(0, |c| c.exit_code());
    if let Some(engine) = controller.and_then(|c| c.into_engine()) {
        engine.shutdown();
    }
    // An orderly exit, even one that cut a capability read short, is not a crash.
    marker.clear();
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// One winit hook for every window: the forced theme (developer flag) and the OSD's no-activate attributes.
fn select_backend(scheme: i32) -> Result<(), slint::PlatformError> {
    let theme = match scheme {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    };
    slint::BackendSelector::new()
        .with_winit_window_attributes_hook(move |attrs: WindowAttributes| {
            let attrs = if theme.is_some() { attrs.with_theme(theme) } else { attrs };
            if attrs.title == osd::TITLE { osd::attributes(attrs) } else { attrs }
        })
        .select()
}
