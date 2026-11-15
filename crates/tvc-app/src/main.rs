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

use std::io::Write;
use std::path::{Path, PathBuf};

use slint::winit_030::winit::window::{Theme, WindowAttributes};
use tvc_core::cli::{self, CliCommand, GetTarget};
use tvc_core::engine::Command;
use tvc_core::hub::{HubConfig, MonitorHub};
use tvc_core::identity::Discovery;
use tvc_core::profile::ProfileStore;
use tvc_core::crash::Marker;
use tvc_core::store::CapabilitiesCache;
use tvc_core::{clock, log};

fn main() -> Result<(), slint::PlatformError> {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| all.iter().any(|a| a == f);
    let scheme = if has("--dark") { 1 } else if has("--light") { 2 } else { 0 };
    let demo = cfg!(debug_assertions) && has("--demo");
    // The developer flags are this exe's own: the parser and a running copy never see them.
    let args: Vec<String> =
        all.iter().filter(|a| !matches!(a.as_str(), "--dark" | "--light") && !(demo && *a == "--demo")).cloned().collect();

    let parsed = match cli::parse(&args) {
        Ok(parsed) => parsed,
        Err(e) => {
            platform::console::attach();
            eprintln!("{}\n\n{}", e.message, cli::help());
            std::process::exit(cli::EXIT_USAGE);
        }
    };
    if parsed.command != CliCommand::Show || parsed.version {
        platform::console::attach(); // so command-line output reaches the calling terminal
    }
    if parsed.command == CliCommand::Help {
        println!("{}", cli::help());
        return Ok(());
    }
    if parsed.version {
        println!("{}", cli::version_line());
        return Ok(());
    }

    let data_dir = if demo { std::env::temp_dir().join("thinkvision-control-demo") } else { platform::data_dir() };
    let _ = std::fs::create_dir_all(&data_dir);
    clock::set_utc_offset_source(localtime::utc_offset);
    log::init(&data_dir);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::write(&format!("Panic: {info}")); // release builds abort right after
        default_hook(info);
    }));

    // Every verb goes to a running copy when there is one, `--get` included, and gets its exit code and output back.
    let listener = if demo {
        None
    } else {
        match platform::instance::start(&args) {
            platform::instance::Start::Forwarded(Some(reply)) => {
                if i32::from(reply.exit) == cli::EXIT_USAGE {
                    eprint!("{}", reply.stdout);
                } else {
                    print!("{}", reply.stdout);
                }
                let _ = std::io::stdout().flush();
                std::process::exit(reply.exit.into());
            }
            platform::instance::Start::Forwarded(None) => {
                let text = "Timed out: the running copy didn't answer";
                log::write(&format!("Forwarding to the running copy: {text}"));
                println!("{}", controller::failure_line(&parsed.command, parsed.json, "TimedOut", text));
                std::process::exit(cli::exit_code(None, true));
            }
            platform::instance::Start::Primary(l) => Some(l),
        }
    };
    select_backend(scheme)?;

    let headless = parsed.command != CliCommand::Show && !demo;
    let mode = match parsed.command {
        CliCommand::Show if parsed.startup => "tray".to_string(),
        CliCommand::Show => "window".to_string(),
        ref command => format!("command line {}", cli::format(command).map(|a| a.join(" ")).unwrap_or_default()),
    };
    log::write(&format!("Start: {}, {mode}{}", cli::version_line(), if demo { ", demo" } else { "" }));

    let profile = ProfileStore::new(&data_dir).profile().clone();
    // A marker left by a process that died during a risky monitor operation starts this run in safe mode. Only
    // dead owners' markers are read. A resident start clears them; a one-shot run leaves them for the app to show.
    let safe_mode = state_dirs(&data_dir).iter().any(|d| !Marker::left_over(d, platform::process_alive).is_empty());
    // No cached capabilities: the connect reads them from the monitor first (not in safe mode).
    let cold = !safe_mode && state_dirs(&data_dir).iter().all(|d| CapabilitiesCache::new(d).load().is_none());
    let discovery: Box<dyn Discovery> = if demo {
        let mut display = tvc_core::fake::FakeDiscovery::display(&profile.edid_id, Some("DEMO"), None);
        display.name = profile.short_name.clone();
        Box::new(tvc_core::fake::FakeDiscovery::new(vec![(display, tvc_core::fake::FakeLink::new())]))
    } else {
        tvc_ddc::discovery()
    };
    let hub = MonitorHub::spawn(
        HubConfig { discovery, root: data_dir.clone(), safe_mode },
        Box::new(|event| {
            let _ = slint::invoke_from_event_loop(move || controller::with(|c| c.on_hub(event)));
        }),
    );
    // After the hub's first rescan, which moves the 0.0.8 markers into their monitor's folder.
    let left_over: Vec<_> =
        state_dirs(&data_dir).iter().flat_map(|d| Marker::left_over(d, platform::process_alive)).collect();
    for stale in &left_over {
        let operation = if stale.operation.is_empty() { "a risky monitor operation" } else { &stale.operation };
        log::write(&format!("Safe mode: an earlier run crashed during {operation}"));
    }
    let app = controller::Controller::new(hub, profile, data_dir.clone(), scheme, demo, left_over);
    controller::install(app.clone());

    if let Some(listener) = listener {
        listener.listen(|args, responder| {
            let _ = slint::invoke_from_event_loop(move || controller::with(|c| c.on_forwarded(args, responder)));
        });
    }

    app.connect_with(match parsed.command {
        // `--get brightness` reads only what it prints.
        CliCommand::Get(target) if headless && target != GetTarget::All => Command::ConnectReading(controller::get_codes(target)),
        _ => Command::Connect,
    });
    let job = controller::CliJob::own(&parsed);
    if headless {
        // A verb with no resident copy: connect once, run it, show the OSD, exit.
        app.start_headless(job, cold);
    } else {
        if parsed.command != CliCommand::Show {
            app.queue_cli(job); // `--demo` with a verb
        }
        // Also starts the retry burst and the display/power event watcher.
        app.start_resident(!(parsed.startup && app.background_mode()));
    }
    drop(app);

    slint::run_event_loop_until_quit()?;

    let controller = controller::uninstall();
    let exit_code = controller.as_ref().map_or(0, |c| c.exit_code());
    if let Some(hub) = controller.as_ref().and_then(|c| c.take_hub()) {
        hub.shutdown();
    }
    // An orderly exit, even one that cut a capability read short, is not a crash.
    for dir in state_dirs(&data_dir) {
        Marker::new(dir, &tvc_core::crash::process_key()).clear();
    }
    log::write(&format!("Exit {exit_code}"));
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// The data directory and every monitor's folder in it: where crash markers and cached capabilities live.
fn state_dirs(data_dir: &Path) -> Vec<PathBuf> {
    let monitors = std::fs::read_dir(data_dir.join("monitors")).into_iter().flatten().flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path());
    std::iter::once(data_dir.to_path_buf()).chain(monitors).collect()
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
