//! Why a write ended the way it did, in a form the CLI can turn into an exit code and the UI into a message.

/// Set on every `WriteResult` by the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResultKind {
    #[default]
    Ok,
    /// A Range write the monitor ignored ("remains"); `WriteResult::kept` has the value it kept.
    Kept,
    /// The safety policy refused it (NeverWrite, unnamed code, needs confirmation, backup failed).
    Refused,
    /// The value isn't one the monitor accepts for this code.
    OutOfRange,
    /// The monitor says it doesn't support the code (never retried).
    Unsupported,
    /// The display is off, the session is locked or the PC is sleeping: no monitor traffic.
    Asleep,
    /// Safe mode after a crash: no monitor writes.
    SafeMode,
    /// No monitor attached.
    NotFound,
    /// Found but not answering, or the connection dropped.
    NotResponding,
}
