//! MI protocol version tracking.
//!
//! framewalk targets GDB/MI version 3 as the primary protocol. A connection
//! starts in [`MiVersion::Unknown`] until the first successful `-gdb-version`
//! or `-list-features` response is observed, at which point the version is
//! inferred from the GDB version string and/or the presence of known
//! capability flags. Step 4 fills in the inference logic; Step 3 just
//! carries the enum.

/// Which GDB/MI protocol version we believe we're talking to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiVersion {
    /// We haven't yet observed enough of GDB's output to infer the version.
    /// Default state on connection open.
    #[default]
    Unknown,
    /// mi2 — GDB 6.0 through ~9.0.
    Mi2,
    /// mi3 — GDB 9.1 and later. The primary target for framewalk.
    Mi3,
}

impl MiVersion {
    /// The interpreter-name string to pass to `gdb --interpreter=...`.
    ///
    /// Returns `"mi3"` for [`MiVersion::Unknown`] since that's the version
    /// framewalk negotiates against by default. Callers that want to run
    /// against an older GDB must explicitly set [`MiVersion::Mi2`].
    #[must_use]
    pub const fn as_interpreter_arg(self) -> &'static str {
        match self {
            Self::Unknown | Self::Mi3 => "mi3",
            Self::Mi2 => "mi2",
        }
    }
}
