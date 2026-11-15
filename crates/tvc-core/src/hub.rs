//! One engine per attached monitor, behind one handle. The hub enumerates displays through [`Discovery`], spawns
//! and stops engines as monitors come and go, keeps each monitor's id stable while it stays attached, and fans
//! `Target::All` commands out as one batch.
//!
//! Contract pinned in Wave 2; the hub itself is implemented in w3-hub.

use std::path::PathBuf;

use crate::engine::{Command, Event, Origin, PowerState, WriteResult};
use crate::identity::{DisplayInfo, Discovery, ListError, MonitorId};

/// Allocated by the hub, monotonic for the life of the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    One(MonitorId),
    All,
}

/// An attached monitor as the hub reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub id: MonitorId,
    pub info: DisplayInfo,
}

pub enum HubCommand {
    /// `Target::All` is accepted only for Set, Step, DragTo, EndDrag and ApplyPreset; anything else with `All` is
    /// refused (a `Batch` with no results). Power, Flush and Shutdown never go through `Send`.
    Send { target: Target, op: OpId, command: Box<Command> },
    /// Enumerate again: spawn engines for new monitors, stop the ones that left.
    Rescan,
    /// Broadcast to every engine.
    Power(PowerState),
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum HubEvent {
    /// The current list after every rescan; replaces `Event::Displays`.
    Monitors(Result<Vec<Monitor>, ListError>),
    Attached(Monitor),
    Detached(MonitorId),
    /// An engine's own event, tagged with its monitor.
    Monitor { id: MonitorId, event: Event },
    /// Every result of one `Target::All` command, once all engines answered.
    Batch { op: OpId, origin: Origin, code: Option<u8>, results: Vec<(MonitorId, WriteResult)> },
}

/// How the hub starts.
pub struct HubConfig {
    pub discovery: Box<dyn Discovery>,
    /// The data directory; each monitor gets `monitors/<id folder>/`.
    pub root: PathBuf,
    pub safe_mode: bool,
}

pub struct MonitorHub;

impl MonitorHub {
    /// Starts the hub thread and runs a first `Rescan`.
    pub fn spawn(config: HubConfig, sink: Box<dyn Fn(HubEvent) + Send>) -> HubHandle {
        let _ = (config, sink);
        todo!("w3-hub")
    }
}

pub struct HubHandle {
    _private: (),
}

impl HubHandle {
    /// A fresh op id for a `Send`.
    pub fn next_op(&self) -> OpId {
        todo!("w3-hub")
    }

    pub fn send(&self, command: HubCommand) {
        let _ = command;
        todo!("w3-hub")
    }

    /// Stops every engine (each waits up to `engine::SHUTDOWN_WAIT`) and the hub thread.
    pub fn shutdown(self) {
        todo!("w3-hub")
    }
}
