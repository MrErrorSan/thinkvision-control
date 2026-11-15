//! D-Bus through zbus (Linux), which Slint's tray already builds in: the session and system bus connections, made
//! once and shared, and the few calls the app makes. Every caller keeps a fallback for when a bus or a service is
//! missing.

use std::sync::Mutex;

use zbus::blocking::Connection;

static SESSION: Mutex<Option<Connection>> = Mutex::new(None);
static SYSTEM: Mutex<Option<Connection>> = Mutex::new(None);

/// The session bus; None when it can't be reached (tried again on the next call).
pub fn session() -> Option<Connection> {
    shared(&SESSION, Connection::session)
}

/// The system bus; None when it can't be reached (tried again on the next call).
pub fn system() -> Option<Connection> {
    shared(&SYSTEM, Connection::system)
}

fn shared(slot: &Mutex<Option<Connection>>, connect: fn() -> zbus::Result<Connection>) -> Option<Connection> {
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_none_or(Connection::is_closed) {
        *slot = connect().ok();
    }
    slot.clone()
}

/// `org.freedesktop.DBus.NameHasOwner(name)`; None when the call fails.
pub fn name_has_owner(bus: &Connection, name: &str) -> Option<bool> {
    let reply = bus
        .call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &name)
        .ok()?;
    reply.body().deserialize::<bool>().ok()
}

/// Whether a StatusNotifier host (tray) is on the session bus. None = unknown (no session bus).
pub fn tray_host_present() -> Option<bool> {
    name_has_owner(&session()?, "org.kde.StatusNotifierWatcher")
}
