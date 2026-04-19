//! The [`Frame`] enum — the unit of output the framer emits.

/// A single event produced by the [`Framer`](crate::Framer) from the raw
/// byte stream of a GDB/MI subprocess.
///
/// A `Frame` borrows from the framer's internal buffer, so the caller must
/// finish handling one frame before requesting the next via
/// [`Framer::pop`](crate::Framer::pop) or pushing more bytes via
/// [`Framer::push`](crate::Framer::push). The borrow checker enforces this
/// statically; there is no way to misuse it at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame<'a> {
    /// A complete MI output line, with any trailing `\n` or `\r\n` stripped.
    ///
    /// The byte contents are delivered verbatim: no grammar interpretation,
    /// no escape decoding, no UTF-8 validation. That's the codec layer's job.
    /// An embedded lone `\r` (one that is not followed by `\n`) is preserved
    /// as raw content.
    ///
    /// A `Line` is never the GDB/MI response-group terminator: when GDB emits
    /// a line whose content is `(gdb)` (with optional trailing ASCII
    /// whitespace), the framer yields [`Frame::GroupTerminator`] instead.
    ///
    /// The empty line is a valid `Line(b"")`.
    Line(&'a [u8]),

    /// The `(gdb)` line that terminates a response group in the MI protocol.
    ///
    /// Per the GDB manual, this marker sits after the optional result record
    /// of each response, and its presence tells a frontend that GDB has
    /// finished emitting that response group. The framer recognises `(gdb)`
    /// with optional trailing ASCII spaces or tabs, since real GDB output
    /// sometimes carries a trailing space (a historical carry-over from the
    /// CLI prompt format) even though the BNF in the manual shows none.
    GroupTerminator,
}
