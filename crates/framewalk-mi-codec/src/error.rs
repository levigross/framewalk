//! Parser and encoder errors.
//!
//! `CodecError` carries a byte offset into the input so callers can localise
//! failures precisely — useful for test fixtures, debug tools, and the
//! eventual `dump_mi` example. The error enum is hand-written (no `thiserror`)
//! because `framewalk-mi-codec` commits to zero runtime dependencies.

use alloc::string::String;
use core::fmt;

/// A codec-layer error, carrying the failure kind and the byte offset into
/// the input line at which the failure was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecError {
    /// The specific failure mode.
    pub kind: CodecErrorKind,
    /// Byte offset into the record being parsed at which the error was
    /// detected. For end-of-input errors this equals `input.len()`.
    pub offset: usize,
}

impl CodecError {
    #[inline]
    #[must_use]
    pub const fn new(kind: CodecErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "codec error at byte {}: {}", self.offset, self.kind)
    }
}

impl core::error::Error for CodecError {}

/// The specific kind of parser/encoder failure.
///
/// Variants carry only plain data (no non-`'static` borrows), so the error
/// type is freely cloneable and can be stored long-term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecErrorKind {
    /// The input ended while the parser expected more bytes.
    UnexpectedEnd,

    /// An unexpected byte was seen. `expected` describes what the parser
    /// was looking for; `found` is the actual byte.
    UnexpectedByte { expected: Expected, found: u8 },

    /// A record-prefix byte (`^`, `*`, `+`, `=`, `~`, `@`, `&`, or a digit
    /// starting an optional token) was expected but not seen.
    InvalidRecordPrefix { found: u8 },

    /// A result-class identifier (`done` / `running` / `connected` /
    /// `error` / `exit`) was expected but something else was read.
    UnknownResultClass { name: String },

    /// A stream record (prefix `~`, `@`, or `&`) carried a leading token,
    /// which the GDB/MI BNF forbids.
    StreamRecordHasToken,

    /// A numeric token was syntactically valid (digits) but overflowed
    /// `u64`, which is framewalk's documented maximum token width.
    TokenOverflow,

    /// An identifier (result name / result-class / async-class) was empty
    /// or contained a byte outside the allowed alphabet.
    InvalidIdentifier,

    /// A `result` production was missing its `=`.
    ExpectedEquals,

    /// A list mixed bare values and `name=value` pairs, which the BNF
    /// forbids (a list is either `[value,...]` or `[result,...]`, never
    /// both).
    MixedList,

    /// A c-string failed to decode. See [`CStringError`] for the sub-reason.
    InvalidCString(CStringError),

    /// A c-string's decoded bytes were not valid UTF-8. framewalk decodes
    /// `Value::Const` eagerly into `String` and fails closed on non-UTF-8
    /// input; callers that need binary values must use `mi_raw_command`
    /// and parse the raw line themselves.
    InvalidUtf8,

    /// Bytes remained after the record was fully parsed. A valid MI line
    /// contains exactly one record.
    TrailingGarbage,
}

impl fmt::Display for CodecErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => f.write_str("unexpected end of input"),
            Self::UnexpectedByte { expected, found } => {
                write!(
                    f,
                    "unexpected byte {}: expected {}",
                    byte_repr(*found),
                    expected
                )
            }
            Self::InvalidRecordPrefix { found } => {
                write!(f, "invalid record prefix byte {}", byte_repr(*found))
            }
            Self::UnknownResultClass { name } => {
                write!(f, "unknown result class {name:?}")
            }
            Self::StreamRecordHasToken => {
                f.write_str("stream record carries a leading token, which the MI BNF forbids")
            }
            Self::TokenOverflow => f.write_str("token value does not fit in u64"),
            Self::InvalidIdentifier => f.write_str("invalid or empty identifier"),
            Self::ExpectedEquals => f.write_str("expected '=' after result name"),
            Self::MixedList => {
                f.write_str("list mixes bare values and named results, which the MI BNF forbids")
            }
            Self::InvalidCString(sub) => write!(f, "invalid c-string: {sub}"),
            Self::InvalidUtf8 => f.write_str("c-string contents are not valid UTF-8"),
            Self::TrailingGarbage => f.write_str("unexpected trailing bytes after record"),
        }
    }
}

/// Human-readable description of what the parser expected.
///
/// Kept small on purpose: `Byte` covers single-byte expectations, `OneOf`
/// covers small byte sets, and `Description` is an escape hatch for
/// higher-level expectations ("an identifier", "a hex digit", etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    Byte(u8),
    OneOf(&'static [u8]),
    Description(&'static str),
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Byte(b) => write!(f, "{}", byte_repr(*b)),
            Self::OneOf(bytes) => {
                f.write_str("one of [")?;
                let mut first = true;
                for &b in *bytes {
                    if !first {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", byte_repr(b))?;
                    first = false;
                }
                f.write_str("]")
            }
            Self::Description(d) => f.write_str(d),
        }
    }
}

/// Sub-reasons for a c-string decode failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CStringError {
    /// A `"` was expected at the start of a c-string.
    MissingOpenQuote,
    /// The c-string had an opening `"` but no matching closing `"` before
    /// the record ended.
    MissingCloseQuote,
    /// A backslash was followed by a byte that does not start a valid
    /// escape sequence.
    InvalidEscape { found: u8 },
    /// An escape sequence was truncated (e.g. `\x` with fewer than 2 hex
    /// digits before the closing `"`).
    TruncatedEscape,
    /// A `\xHH` escape contained a non-hex byte.
    InvalidHexDigit { found: u8 },
    /// An `\ooo` escape contained a non-octal byte.
    InvalidOctalDigit { found: u8 },
}

impl fmt::Display for CStringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOpenQuote => f.write_str("missing opening `\"`"),
            Self::MissingCloseQuote => f.write_str("unterminated c-string (missing closing `\"`)"),
            Self::InvalidEscape { found } => {
                write!(f, "invalid escape sequence \\{}", byte_repr(*found))
            }
            Self::TruncatedEscape => f.write_str("truncated escape sequence"),
            Self::InvalidHexDigit { found } => {
                write!(f, "invalid hex digit in \\x escape: {}", byte_repr(*found))
            }
            Self::InvalidOctalDigit { found } => {
                write!(
                    f,
                    "invalid octal digit in \\ooo escape: {}",
                    byte_repr(*found)
                )
            }
        }
    }
}

/// Format a byte for display: printable ASCII is shown in quotes, everything
/// else as `0xHH`. Avoids bringing in a heavyweight hex-encoding dep and
/// gives human-friendly output in error messages.
fn byte_repr(b: u8) -> ByteRepr {
    ByteRepr(b)
}

struct ByteRepr(u8);

impl fmt::Display for ByteRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.0;
        if (0x20..=0x7e).contains(&b) {
            write!(f, "'{}'", b as char)
        } else {
            write!(f, "0x{b:02x}")
        }
    }
}
