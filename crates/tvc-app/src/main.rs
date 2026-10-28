#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! ThinkVision Control. `thinkvision-control [--startup] [--preset day|night] [--input hdmi|dp|usbc|this]
//! [--step brightness=±N|contrast=±N]`, plus developer flags: `--dark` / `--light`, and in debug builds only
//! `--demo` (a simulated monitor, data in the temp folder).
slint::include_modules!();

mod clipboard;
mod controller;
mod osd;
mod platform;
mod tray;
mod view;

use slint::winit_030::winit::window::{Theme, WindowAttributes};
use tvc_core::cli::{self, CliCommand};
use tvc_core::engine::{Command, Engine};
use tvc_core::link::{MonitorLink, MonitorMatch};
use tvc_core::profile::ProfileStore;
use tvc_core::{clock, log};

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let scheme = if has("--dark") { 1 } else if has("--light") { 2 } else { 0 };
    select_backend(scheme)?;

    let demo = cfg!(debug_assertions) && has("--demo");
    let data_dir = if demo { std::env::temp_dir().join("thinkvision-control-demo") } else { platform::data_dir() };
    let _ = std::fs::create_dir_all(&data_dir);
    clock::set_utc_offset(local_utc_offset());
    log::init(&data_dir);

    let parsed = cli::parse(&args);
    let listener = if demo {
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
    let engine = Engine::spawn(
        link,
        profiles,
        data_dir.clone(),
        profile.timing.clone(),
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
        // A verb with no resident copy: connect once, run it, show the OSD, exit (INV §6.2).
        app.start_headless(parsed.command);
    } else {
        // Also starts the retry burst and the display/power event watcher.
        app.start_resident(!(parsed.startup && app.background_mode()));
    }
    drop(app);

    slint::run_event_loop_until_quit()?;

    if let Some(engine) = controller::uninstall().and_then(|c| c.into_engine()) {
        engine.shutdown();
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

/// Seconds east of UTC, for backup names and log lines (tvc-core makes no OS calls).
#[cfg(windows)]
fn local_utc_offset() -> i32 {
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: [u16; 8],
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: [u16; 8],
        daylight_bias: i32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTimeZoneInformation(tzi: *mut TimeZoneInformation) -> u32;
    }
    // SAFETY: GetTimeZoneInformation fills the struct it is given; the layout matches TIME_ZONE_INFORMATION.
    let (mode, tzi) = unsafe {
        let mut tzi: TimeZoneInformation = std::mem::zeroed();
        (GetTimeZoneInformation(&mut tzi), tzi)
    };
    let extra = match mode {
        1 => tzi.standard_bias,
        2 => tzi.daylight_bias,
        _ => 0,
    };
    -(tzi.bias + extra) * 60
}

#[cfg(unix)]
fn local_utc_offset() -> i32 {
    // SAFETY: localtime_r writes one `tm` for the given time.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
}

#[cfg(not(any(windows, unix)))]
fn local_utc_offset() -> i32 {
    0
}
