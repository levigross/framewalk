//! The byte-level framer. See [`Framer`] for details.

use alloc::vec::Vec;

use crate::frame::Frame;

/// Accumulates bytes from a GDB/MI subprocess and yields complete [`Frame`]s.
///
/// The framer is push-based on the input side — call [`push`](Self::push)
/// with whatever bytes arrive from the transport, chunked however the OS
/// chose to deliver them — and pull-based on the output side — call
/// [`pop`](Self::pop) in a loop until it returns `None`. It performs no
/// I/O, owns no threads, and is runtime-agnostic and `no_std`-compatible.
///
/// Line terminators `\n` and `\r\n` are both accepted and stripped from the
/// emitted [`Frame::Line`] contents. A lone `\r` without a following `\n` is
/// treated as raw content and preserved inside the line. The framer does
/// **not** understand MI grammar — it only delimits lines and recognises the
/// `(gdb)` response-group terminator. Everything else is the codec's job.
///
/// # Borrow semantics
///
/// Each [`Frame`] returned by [`pop`](Self::pop) borrows from the framer's
/// internal buffer, which means the caller must finish with one frame before
/// calling `pop` or `push` again. The borrow checker enforces this
/// statically:
///
/// ```compile_fail
/// # use framewalk_mi_wire::Framer;
/// let mut framer = Framer::new();
/// framer.push(b"a\nb\n");
/// let a = framer.pop();
/// let b = framer.pop(); // ERROR: cannot borrow `framer` mutably twice
/// # let _ = (a, b);
/// ```
///
/// The idiomatic usage loops over frames with a fresh borrow each iteration:
///
/// ```
/// # use framewalk_mi_wire::{Framer, Frame};
/// let mut framer = Framer::new();
/// framer.push(b"^done\n(gdb)\n");
/// while let Some(frame) = framer.pop() {
///     match frame {
///         Frame::Line(bytes) => {
///             // feed bytes to the codec layer
///             let _ = bytes;
///         }
///         Frame::GroupTerminator => {
///             // response group complete
///         }
///     }
/// }
/// ```
///
/// # Memory
///
/// The framer holds all received-but-not-yet-consumed bytes in an internal
/// `Vec<u8>`. Buffer space is reclaimed lazily during [`push`](Self::push)
/// once the read cursor has advanced past the midpoint of the buffer, which
/// bounds amortised memory to roughly twice the largest in-flight line. If
/// the caller wants a hard cap (e.g. to survive a wedged GDB that never
/// emits a newline), it can watch [`pending_bytes`](Self::pending_bytes)
/// and drop the framer when the threshold is crossed; the framer itself
/// imposes no limit.
#[derive(Debug, Default)]
pub struct Framer {
    /// Received bytes not yet consumed by a completed frame.
    ///
    /// Bytes before `read_pos` are logically consumed but kept around so
    /// that [`pop`](Self::pop) can hand out borrowed slices without having
    /// to shift everything on every call. Compaction happens lazily.
    buf: Vec<u8>,

    /// Index into `buf` where the next unconsumed byte begins.
    read_pos: usize,
}

impl Framer {
    /// Create an empty framer with no preallocated buffer.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty framer, preallocating `cap` bytes of buffer space.
    ///
    /// Useful when the caller has a rough idea of expected MI line sizes
    /// (a typical MI line is tens to a few hundred bytes, but
    /// `-data-read-memory-bytes` and friends can emit much larger lines).
    #[inline]
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            read_pos: 0,
        }
    }

    /// Append bytes received from the transport.
    ///
    /// This never fails. It may allocate to grow the internal buffer;
    /// otherwise it is a cheap `extend_from_slice`.
    pub fn push(&mut self, bytes: &[u8]) {
        // Compact lazily: once the read cursor has advanced past the
        // midpoint of the buffer, reclaim the consumed prefix. This bounds
        // amortised memory to ~2x the largest in-flight line without
        // shifting on every `pop`.
        if self.read_pos != 0 && self.read_pos >= self.buf.len() / 2 {
            self.compact();
        }
        self.buf.extend_from_slice(bytes);
    }

    /// Pull the next complete frame, if one is available.
    ///
    /// The returned [`Frame`] borrows from the framer's internal buffer, so
    /// callers must finish with it before the next call to
    /// [`push`](Self::push) or [`pop`](Self::pop). Returns `None` when the
    /// buffer does not yet contain a complete line (no `\n` has been seen
    /// since the last frame).
    pub fn pop(&mut self) -> Option<Frame<'_>> {
        // Scan forward from `read_pos` for the next '\n'. No `memchr` dep —
        // `slice::iter().position` is the zero-dep equivalent and is more
        // than adequate for the byte volumes MI traffic produces.
        let tail = &self.buf[self.read_pos..];
        let rel_newline = tail.iter().position(|&b| b == b'\n')?;
        let abs_newline = self.read_pos + rel_newline;

        // Strip a trailing '\r' if we have CRLF. Only strip one — a lone
        // '\r' embedded in the middle of a line is raw content.
        let line_end = if abs_newline > self.read_pos && self.buf[abs_newline - 1] == b'\r' {
            abs_newline - 1
        } else {
            abs_newline
        };

        let line_start = self.read_pos;
        // Advance past the '\n' *before* creating the immutable borrow of
        // `self.buf`, so the mutation to `self.read_pos` and the borrow
        // don't overlap.
        self.read_pos = abs_newline + 1;

        let line = &self.buf[line_start..line_end];
        Some(if is_group_terminator(line) {
            Frame::GroupTerminator
        } else {
            Frame::Line(line)
        })
    }

    /// Number of bytes buffered but not yet yielded as a frame.
    ///
    /// Useful for detecting runaway input from a misbehaving GDB (a GDB that
    /// never emits a newline would grow this unboundedly, which the framer
    /// itself will not stop — the caller must).
    #[inline]
    #[must_use]
    pub fn pending_bytes(&self) -> usize {
        self.buf.len() - self.read_pos
    }

    /// Discard the consumed prefix of the buffer and reset the read cursor.
    fn compact(&mut self) {
        if self.read_pos == 0 {
            return;
        }
        self.buf.drain(..self.read_pos);
        self.read_pos = 0;
    }
}

/// Returns `true` if `line` is the MI response-group terminator.
///
/// The strict form per the BNF in the GDB manual is `(gdb)` exactly, but
/// real GDB output sometimes carries trailing ASCII whitespace (a carry-over
/// from the CLI prompt format). We accept `(gdb)` followed by any number of
/// ASCII spaces or tabs — and nothing else — as the terminator.
fn is_group_terminator(line: &[u8]) -> bool {
    const MARKER: &[u8] = b"(gdb)";
    if !line.starts_with(MARKER) {
        return false;
    }
    line[MARKER.len()..]
        .iter()
        .all(|&b| b == b' ' || b == b'\t')
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Owned mirror of [`Frame`] so tests can collect a whole sequence into a
    /// `Vec` without fighting the borrow checker over multiple live frames.
    #[derive(Debug, PartialEq, Eq)]
    enum OwnedFrame {
        Line(Vec<u8>),
        Term,
    }

    impl From<Frame<'_>> for OwnedFrame {
        fn from(f: Frame<'_>) -> Self {
            match f {
                Frame::Line(b) => Self::Line(b.to_vec()),
                Frame::GroupTerminator => Self::Term,
            }
        }
    }

    fn line(s: &[u8]) -> OwnedFrame {
        OwnedFrame::Line(s.to_vec())
    }

    fn term() -> OwnedFrame {
        OwnedFrame::Term
    }

    /// Drain every frame currently available.
    fn drain(framer: &mut Framer) -> Vec<OwnedFrame> {
        let mut out = Vec::new();
        while let Some(f) = framer.pop() {
            out.push(f.into());
        }
        out
    }

    // ---- Basic ----

    #[test]
    fn new_is_empty() {
        let mut f = Framer::new();
        assert!(f.pop().is_none());
        assert_eq!(f.pending_bytes(), 0);
    }

    #[test]
    fn with_capacity_does_not_affect_semantics() {
        let mut f = Framer::with_capacity(1024);
        f.push(b"hello\n");
        assert_eq!(drain(&mut f), [line(b"hello")]);
    }

    #[test]
    fn push_empty_is_noop() {
        let mut f = Framer::new();
        f.push(b"");
        assert!(f.pop().is_none());
        assert_eq!(f.pending_bytes(), 0);
    }

    #[test]
    fn pop_on_empty_returns_none() {
        let mut f = Framer::new();
        assert!(f.pop().is_none());
    }

    // ---- Single-line happy paths ----

    #[test]
    fn single_line_lf() {
        let mut f = Framer::new();
        f.push(b"^done\n");
        assert_eq!(drain(&mut f), [line(b"^done")]);
    }

    #[test]
    fn single_line_crlf() {
        let mut f = Framer::new();
        f.push(b"^done\r\n");
        assert_eq!(drain(&mut f), [line(b"^done")]);
    }

    #[test]
    fn empty_line_lf() {
        let mut f = Framer::new();
        f.push(b"\n");
        assert_eq!(drain(&mut f), [line(b"")]);
    }

    #[test]
    fn empty_line_crlf() {
        let mut f = Framer::new();
        f.push(b"\r\n");
        assert_eq!(drain(&mut f), [line(b"")]);
    }

    #[test]
    fn multiple_empty_lines() {
        let mut f = Framer::new();
        f.push(b"\n\n\n");
        assert_eq!(drain(&mut f), [line(b""), line(b""), line(b"")]);
    }

    // ---- Multi-line batches ----

    #[test]
    fn two_lines_one_push_lf() {
        let mut f = Framer::new();
        f.push(b"^done\n*stopped\n");
        assert_eq!(drain(&mut f), [line(b"^done"), line(b"*stopped")]);
    }

    #[test]
    fn two_lines_one_push_crlf() {
        let mut f = Framer::new();
        f.push(b"^done\r\n*stopped\r\n");
        assert_eq!(drain(&mut f), [line(b"^done"), line(b"*stopped")]);
    }

    #[test]
    fn mixed_terminators_in_sequence() {
        let mut f = Framer::new();
        f.push(b"foo\nbar\r\nbaz\n");
        assert_eq!(drain(&mut f), [line(b"foo"), line(b"bar"), line(b"baz")]);
    }

    // ---- Streaming / split pushes ----

    #[test]
    fn line_split_across_two_pushes() {
        let mut f = Framer::new();
        f.push(b"^don");
        assert!(f.pop().is_none());
        f.push(b"e\n");
        assert_eq!(drain(&mut f), [line(b"^done")]);
    }

    #[test]
    fn line_split_byte_by_byte() {
        let mut f = Framer::new();
        for &b in b"^done\n" {
            f.push(&[b]);
        }
        assert_eq!(drain(&mut f), [line(b"^done")]);
    }

    #[test]
    fn crlf_split_across_pushes() {
        let mut f = Framer::new();
        f.push(b"^done\r");
        // No \n yet — no frame available.
        assert!(f.pop().is_none());
        f.push(b"\n");
        assert_eq!(drain(&mut f), [line(b"^done")]);
    }

    #[test]
    fn line_and_terminator_arrive_separately() {
        let mut f = Framer::new();
        f.push(b"^done\n");
        assert_eq!(drain(&mut f), [line(b"^done")]);
        f.push(b"(gdb)\n");
        assert_eq!(drain(&mut f), [term()]);
    }

    #[test]
    fn partial_line_yields_nothing() {
        let mut f = Framer::new();
        let input: &[u8] = b"^done,msg=\"no newline yet\"";
        f.push(input);
        assert!(f.pop().is_none());
        assert_eq!(f.pending_bytes(), input.len());
    }

    // ---- Group terminator recognition ----

    #[test]
    fn group_terminator_bare() {
        let mut f = Framer::new();
        f.push(b"(gdb)\n");
        assert_eq!(drain(&mut f), [term()]);
    }

    #[test]
    fn group_terminator_trailing_space() {
        let mut f = Framer::new();
        f.push(b"(gdb) \n");
        assert_eq!(drain(&mut f), [term()]);
    }

    #[test]
    fn group_terminator_trailing_multi_space() {
        let mut f = Framer::new();
        f.push(b"(gdb)   \n");
        assert_eq!(drain(&mut f), [term()]);
    }

    #[test]
    fn group_terminator_trailing_tab() {
        let mut f = Framer::new();
        f.push(b"(gdb)\t\n");
        assert_eq!(drain(&mut f), [term()]);
    }

    #[test]
    fn group_terminator_with_crlf() {
        let mut f = Framer::new();
        f.push(b"(gdb)\r\n");
        assert_eq!(drain(&mut f), [term()]);
    }

    // ---- Near-miss terminators (must stay as Line) ----

    #[test]
    fn not_terminator_suffix_alnum() {
        let mut f = Framer::new();
        f.push(b"(gdb)foo\n");
        assert_eq!(drain(&mut f), [line(b"(gdb)foo")]);
    }

    #[test]
    fn not_terminator_leading_space() {
        let mut f = Framer::new();
        f.push(b" (gdb)\n");
        assert_eq!(drain(&mut f), [line(b" (gdb)")]);
    }

    #[test]
    fn not_terminator_missing_close_paren() {
        let mut f = Framer::new();
        f.push(b"(gdb\n");
        assert_eq!(drain(&mut f), [line(b"(gdb")]);
    }

    #[test]
    fn not_terminator_uppercase() {
        let mut f = Framer::new();
        f.push(b"(GDB)\n");
        assert_eq!(drain(&mut f), [line(b"(GDB)")]);
    }

    #[test]
    fn not_terminator_gdb_inside_stream_record() {
        // A console-stream record whose content *mentions* "(gdb)" in its
        // payload must be yielded as a Line, because the framer only matches
        // the terminator at whole-line granularity. The content is preserved
        // verbatim for the codec layer.
        let mut f = Framer::new();
        f.push(b"~\"the prompt is (gdb) btw\"\n");
        assert_eq!(drain(&mut f), [line(b"~\"the prompt is (gdb) btw\"")]);
    }

    // ---- Embedded \r and \n edge cases ----

    #[test]
    fn embedded_cr_is_preserved() {
        // A lone '\r' not followed by '\n' is not a line terminator.
        let mut f = Framer::new();
        f.push(b"foo\rbar\n");
        assert_eq!(drain(&mut f), [line(b"foo\rbar")]);
    }

    #[test]
    fn cr_followed_by_non_lf() {
        let mut f = Framer::new();
        f.push(b"foo\rX\n");
        assert_eq!(drain(&mut f), [line(b"foo\rX")]);
    }

    // ---- Binary / non-ASCII content (framer is byte-level) ----

    #[test]
    fn binary_bytes_in_line() {
        let mut f = Framer::new();
        f.push(b"\x00\xff\x7f\n");
        assert_eq!(drain(&mut f), [line(b"\x00\xff\x7f")]);
    }

    #[test]
    fn utf8_multibyte_in_line() {
        // The framer is byte-level; it does not care about UTF-8.
        let mut f = Framer::new();
        f.push("path=\"café\"\n".as_bytes());
        assert_eq!(drain(&mut f), [line("path=\"café\"".as_bytes())]);
    }

    // ---- Pending bytes ----

    #[test]
    fn pending_bytes_tracks_buffered_input() {
        let mut f = Framer::new();
        assert_eq!(f.pending_bytes(), 0);
        f.push(b"abc");
        assert_eq!(f.pending_bytes(), 3);
        f.push(b"def");
        assert_eq!(f.pending_bytes(), 6);
        f.push(b"\n");
        // Still 7 bytes pending until we actually pop.
        assert_eq!(f.pending_bytes(), 7);
        let _ = f.pop();
        assert_eq!(f.pending_bytes(), 0);
    }

    // ---- Compaction / long-running behaviour ----

    #[test]
    fn many_pushes_and_pops_stay_correct() {
        // Drive enough traffic through the framer that internal compaction
        // triggers several times, and assert we never lose or reorder data.
        let mut f = Framer::new();
        let mut expected: Vec<OwnedFrame> = Vec::new();
        for i in 0..500usize {
            // Alternate between normal lines and group terminators.
            if i % 7 == 0 {
                f.push(b"(gdb)\n");
                expected.push(term());
            } else {
                // Vary line lengths so compaction offsets shift around.
                let pad = i % 40;
                let mut payload = Vec::new();
                payload.extend_from_slice(b"^done,n=\"");
                payload.resize(payload.len() + pad, b'x');
                payload.extend_from_slice(b"\"");
                let mut pushed = payload.clone();
                pushed.push(b'\n');
                f.push(&pushed);
                expected.push(OwnedFrame::Line(payload));
            }
            // Drain intermittently to force read_pos to advance past the
            // midpoint and trigger compact().
            if i % 3 == 0 {
                let drained = drain(&mut f);
                let expected_drained: Vec<_> = expected.drain(..drained.len()).collect();
                assert_eq!(drained, expected_drained);
            }
        }
        // Drain whatever's left.
        assert_eq!(drain(&mut f), expected);
    }

    #[test]
    fn compaction_then_resume() {
        let mut f = Framer::new();
        // Fill, drain, re-fill — compaction should make this transparent.
        for _ in 0..10 {
            f.push(b"line\n");
            let _ = f.pop();
        }
        f.push(b"final\n");
        assert_eq!(drain(&mut f), [line(b"final")]);
        assert_eq!(f.pending_bytes(), 0);
    }

    // ---- Realistic MI response group ----

    #[test]
    fn realistic_mi_response_group() {
        // A typical output group: a console stream record, a result record,
        // and the closing prompt. Arrives in two chunks to exercise splits.
        let mut f = Framer::new();
        f.push(b"~\"Reading symbols...\\n\"\n^done\n");
        f.push(b"(gdb)\n");
        assert_eq!(
            drain(&mut f),
            [
                line(b"~\"Reading symbols...\\n\"" as &[u8]),
                line(b"^done"),
                term(),
            ]
        );
    }

    #[test]
    fn realistic_mi_response_group_crlf() {
        // Same shape, CRLF line endings.
        let mut f = Framer::new();
        f.push(b"~\"Reading symbols...\\n\"\r\n^done\r\n(gdb)\r\n");
        assert_eq!(
            drain(&mut f),
            [
                line(b"~\"Reading symbols...\\n\"" as &[u8]),
                line(b"^done"),
                term(),
            ]
        );
    }

    // ---- is_group_terminator unit tests (function-level coverage) ----

    #[test]
    fn is_terminator_exact() {
        assert!(is_group_terminator(b"(gdb)"));
    }

    #[test]
    fn is_terminator_trailing_whitespace() {
        assert!(is_group_terminator(b"(gdb) "));
        assert!(is_group_terminator(b"(gdb)  "));
        assert!(is_group_terminator(b"(gdb)\t"));
        assert!(is_group_terminator(b"(gdb) \t "));
    }

    #[test]
    fn is_not_terminator() {
        assert!(!is_group_terminator(b""));
        assert!(!is_group_terminator(b"(gdb"));
        assert!(!is_group_terminator(b"gdb)"));
        assert!(!is_group_terminator(b" (gdb)"));
        assert!(!is_group_terminator(b"(gdb)x"));
        assert!(!is_group_terminator(b"(GDB)"));
        assert!(!is_group_terminator(b"(gdb)\n"));
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------
//
// These cover the same territory as a cargo-fuzz target — arbitrary bytes,
// arbitrary chunk splits — but run on stable in `cargo test` with no nightly
// toolchain required. A coverage-guided libFuzzer target will be added in
// Step 8 (hardening) where it can share infrastructure with the codec and
// protocol fuzz targets.
#[cfg(test)]
mod proptests {
    use super::*;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    /// Oracle: given a raw input buffer, compute the exact byte sequence we
    /// expect a correct framer to have produced once it has drained every
    /// complete line out of that buffer. Lines are rejoined with '\n' and
    /// `(gdb)` terminators are canonicalised (any trailing whitespace is
    /// stripped, matching the framer's lossy recognition).
    fn expected_popped_bytes(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = input[cursor..].iter().position(|&b| b == b'\n') {
            let abs = cursor + rel;
            let end = if abs > cursor && input[abs - 1] == b'\r' {
                abs - 1
            } else {
                abs
            };
            let line = &input[cursor..end];
            if is_group_terminator(line) {
                out.extend_from_slice(b"(gdb)");
            } else {
                out.extend_from_slice(line);
            }
            out.push(b'\n');
            cursor = abs + 1;
        }
        out
    }

    /// Oracle: count of bytes that remain unconsumed (i.e., the suffix after
    /// the last '\n', if any).
    fn expected_pending(input: &[u8]) -> usize {
        match input.iter().rposition(|&b| b == b'\n') {
            Some(idx) => input.len() - (idx + 1),
            None => input.len(),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2048,
            .. ProptestConfig::default()
        })]

        /// The central invariant of the framer:
        ///
        /// For any byte sequence split into any chunks, feeding the chunks
        /// to `push` one at a time (with `pop` drains between them) yields
        /// a frame sequence whose contents, rejoined with '\n' terminators,
        /// equal `expected_popped_bytes(input)`; and the final `pending_bytes`
        /// equals `expected_pending(input)`.
        ///
        /// This covers: partial lines across push boundaries, CRLF split
        /// exactly at the '\r'/'\n' boundary, group-terminator recognition,
        /// embedded carriage returns, binary content, and the lazy compaction
        /// logic (which only runs once read_pos crosses the midpoint, so long
        /// traces exercise it repeatedly).
        #[test]
        fn framer_preserves_input_over_arbitrary_chunking(
            input in proptest::collection::vec(any::<u8>(), 0..2048),
            chunk_sizes in proptest::collection::vec(1usize..64, 1..64),
        ) {
            let mut f = Framer::new();
            let mut reconstructed: Vec<u8> = Vec::new();

            let mut offset = 0usize;
            let mut chunks = chunk_sizes.iter().cycle();
            while offset < input.len() {
                let size = *chunks.next().unwrap();
                let end = (offset + size).min(input.len());
                f.push(&input[offset..end]);
                offset = end;
                while let Some(frame) = f.pop() {
                    match frame {
                        Frame::Line(bytes) => {
                            reconstructed.extend_from_slice(bytes);
                            reconstructed.push(b'\n');
                        }
                        Frame::GroupTerminator => {
                            reconstructed.extend_from_slice(b"(gdb)");
                            reconstructed.push(b'\n');
                        }
                    }
                }
            }

            // Final drain — some traces produce their last complete line
            // on the same push as their tail, so pop once more.
            while let Some(frame) = f.pop() {
                match frame {
                    Frame::Line(bytes) => {
                        reconstructed.extend_from_slice(bytes);
                        reconstructed.push(b'\n');
                    }
                    Frame::GroupTerminator => {
                        reconstructed.extend_from_slice(b"(gdb)");
                        reconstructed.push(b'\n');
                    }
                }
            }

            prop_assert_eq!(&reconstructed, &expected_popped_bytes(&input));
            prop_assert_eq!(f.pending_bytes(), expected_pending(&input));
        }

        /// The framer must never panic on any input, however pathological.
        /// This is implied by the previous test but stated explicitly so a
        /// counter-example shrinks to the smallest panicking input rather
        /// than the smallest equality-violating input.
        #[test]
        fn framer_never_panics(
            input in proptest::collection::vec(any::<u8>(), 0..4096),
        ) {
            let mut f = Framer::new();
            f.push(&input);
            while f.pop().is_some() {}
        }
    }
}
