//! Protocol-layer errors.
//!
//! `ProtocolError` wraps lower-layer codec failures and adds a small set of
//! protocol-specific failure modes (malformed command submission, state
//! invariant violation). Parse errors on individual wire records do **not**
//! raise `ProtocolError` — they are surfaced as
//! [`Event::ParseError`](crate::event::Event::ParseError) so a single bad
//! line does not kill the whole session.

use framewalk_mi_codec::CodecError;
use thiserror::Error;

/// Errors returned from `Connection` methods that cannot simply be surfaced
/// as an event.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// A command submission contained invalid UTF-8, an empty operation
    /// name, or other structural defect the encoder refused to serialise.
    #[error("invalid command submission: {reason}")]
    InvalidCommand { reason: &'static str },

    /// The framer's internal buffer grew beyond an allocator-imposed
    /// practical limit (currently unused; reserved for Step 8 hardening
    /// when a configurable max-pending-bytes limit lands).
    #[error("framer buffer exceeded the configured limit")]
    BufferOverflow,

    /// A wire record that parsed successfully but whose content violated
    /// a hard protocol invariant (e.g., a result record claimed `^done`
    /// but referenced a token the state machine never issued AND the
    /// caller has opted into strict correlation). Reserved for future
    /// use; Step 3 treats all untokened and unknown-tokened results as
    /// recoverable events.
    #[error("protocol invariant violated: {reason}")]
    InvariantViolation { reason: &'static str },
}

/// Wrapper type that lets `Event::ParseError` carry a codec failure plus
/// the raw bytes that triggered it, so a caller can log the offending
/// line for diagnostics.
#[derive(Debug, Clone)]
pub struct ParseFailure {
    pub error: CodecError,
    pub raw_line: Vec<u8>,
}
