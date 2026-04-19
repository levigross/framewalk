//! MI3 domain enums — typed representations of GDB/MI value sets.
//!
//! These model constraints the GDB manual defines as fixed sets of valid
//! values. Exposing them as Rust enums means invalid inputs become compile
//! errors (for internal callers) or serde deserialisation errors (for
//! external callers via MCP) rather than GDB rejections at runtime.
//!
//! Every variant's doc comment quotes or paraphrases the GDB manual.
//! See the sourceware.org MI chapter for the canonical definitions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How to print variable values in stack-list-locals,
/// stack-list-arguments, stack-list-variables, and trace-frame-collected.
///
/// Per the manual: 0 = names only, 1 = all values, 2 = simple values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PrintValues {
    /// Names only (no values or types).
    NoValues,
    /// Names and values for all variables.
    AllValues,
    /// Names, types, and values for simple scalar types;
    /// names and types only for aggregate types.
    SimpleValues,
}

impl PrintValues {
    /// The MI command-line form: `--no-values`, `--all-values`, or
    /// `--simple-values`.
    #[must_use]
    pub const fn as_mi_arg(self) -> &'static str {
        match self {
            Self::NoValues => "--no-values",
            Self::AllValues => "--all-values",
            Self::SimpleValues => "--simple-values",
        }
    }
}

/// Register value format for data-list-register-values and
/// trace-frame-collected.
///
/// Per the manual: x=hex, o=octal, t=binary, d=decimal, r=raw, N=natural.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RegisterFormat {
    Hex,
    Octal,
    Binary,
    Decimal,
    Raw,
    Natural,
}

impl RegisterFormat {
    /// The single-character MI format code.
    #[must_use]
    pub const fn as_mi_arg(self) -> &'static str {
        match self {
            Self::Hex => "x",
            Self::Octal => "o",
            Self::Binary => "t",
            Self::Decimal => "d",
            Self::Raw => "r",
            Self::Natural => "N",
        }
    }
}

/// Disassembly opcodes display mode for data-disassemble `--opcodes`.
///
/// Per the manual: none, bytes, or display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum OpcodeMode {
    /// No opcodes shown.
    None,
    /// Show opcodes as raw bytes.
    Bytes,
    /// Show opcodes in display format.
    Display,
}

impl OpcodeMode {
    #[must_use]
    pub const fn as_mi_arg(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bytes => "bytes",
            Self::Display => "display",
        }
    }
}

/// Watchpoint access type for `-break-watch`.
///
/// Per the manual: default is write, `-r` is read, `-a` is access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum WatchType {
    /// Stop on write (default).
    Write,
    /// Stop on read (`-break-watch -r`).
    Read,
    /// Stop on read or write (`-break-watch -a`).
    Access,
}

/// Variable object display format for `-var-set-format`.
///
/// Per the manual: `binary`, `decimal`, `hexadecimal`, `octal`,
/// `natural`, `zero-hexadecimal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum VarFormat {
    Binary,
    Decimal,
    Hexadecimal,
    Octal,
    Natural,
    ZeroHexadecimal,
}

impl VarFormat {
    /// The MI command-line form.
    #[must_use]
    pub const fn as_mi_arg(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Decimal => "decimal",
            Self::Hexadecimal => "hexadecimal",
            Self::Octal => "octal",
            Self::Natural => "natural",
            Self::ZeroHexadecimal => "zero-hexadecimal",
        }
    }
}

/// Memory word format for the deprecated `-data-read-memory` command.
///
/// Per the manual: `x`=hex, `d`=decimal, `o`=octal, `t`=binary,
/// `f`=float, `c`=character, `s`=string, `a`=address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum MemoryWordFormat {
    Hex,
    Decimal,
    Octal,
    Binary,
    Float,
    Character,
    String,
    Address,
}

impl MemoryWordFormat {
    /// The single-character MI format code.
    #[must_use]
    pub const fn as_mi_arg(self) -> &'static str {
        match self {
            Self::Hex => "x",
            Self::Decimal => "d",
            Self::Octal => "o",
            Self::Binary => "t",
            Self::Float => "f",
            Self::Character => "c",
            Self::String => "s",
            Self::Address => "a",
        }
    }
}

/// Trace-find mode for `-trace-find`.
///
/// Each variant carries the parameters that mode requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TraceFindMode {
    /// Stop looking at trace frames.
    None,
    /// Select trace frame by frame number.
    FrameNumber { number: u32 },
    /// Select trace frame by tracepoint number.
    TracepointNumber { number: u32 },
    /// Select trace frame by PC address.
    Pc { address: String },
    /// Select trace frame where PC is inside the given range.
    PcInsideRange { start: String, end: String },
    /// Select trace frame where PC is outside the given range.
    PcOutsideRange { start: String, end: String },
    /// Select trace frame by source line.
    Line { location: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- PrintValues ----

    #[test]
    fn print_values_maps_to_manual_flag_strings() {
        assert_eq!(PrintValues::NoValues.as_mi_arg(), "--no-values");
        assert_eq!(PrintValues::AllValues.as_mi_arg(), "--all-values");
        assert_eq!(PrintValues::SimpleValues.as_mi_arg(), "--simple-values");
    }

    // ---- RegisterFormat ----

    #[test]
    fn register_format_single_char_codes() {
        assert_eq!(RegisterFormat::Hex.as_mi_arg(), "x");
        assert_eq!(RegisterFormat::Octal.as_mi_arg(), "o");
        assert_eq!(RegisterFormat::Binary.as_mi_arg(), "t");
        assert_eq!(RegisterFormat::Decimal.as_mi_arg(), "d");
        assert_eq!(RegisterFormat::Raw.as_mi_arg(), "r");
        assert_eq!(RegisterFormat::Natural.as_mi_arg(), "N");
    }

    // ---- OpcodeMode ----

    #[test]
    fn opcode_mode_matches_manual_names() {
        assert_eq!(OpcodeMode::None.as_mi_arg(), "none");
        assert_eq!(OpcodeMode::Bytes.as_mi_arg(), "bytes");
        assert_eq!(OpcodeMode::Display.as_mi_arg(), "display");
    }

    // ---- VarFormat ----

    #[test]
    fn var_format_matches_manual_names() {
        assert_eq!(VarFormat::Binary.as_mi_arg(), "binary");
        assert_eq!(VarFormat::Decimal.as_mi_arg(), "decimal");
        assert_eq!(VarFormat::Hexadecimal.as_mi_arg(), "hexadecimal");
        assert_eq!(VarFormat::Octal.as_mi_arg(), "octal");
        assert_eq!(VarFormat::Natural.as_mi_arg(), "natural");
        assert_eq!(VarFormat::ZeroHexadecimal.as_mi_arg(), "zero-hexadecimal");
    }

    // ---- MemoryWordFormat ----

    #[test]
    fn memory_word_format_single_char_codes() {
        assert_eq!(MemoryWordFormat::Hex.as_mi_arg(), "x");
        assert_eq!(MemoryWordFormat::Decimal.as_mi_arg(), "d");
        assert_eq!(MemoryWordFormat::Octal.as_mi_arg(), "o");
        assert_eq!(MemoryWordFormat::Binary.as_mi_arg(), "t");
        assert_eq!(MemoryWordFormat::Float.as_mi_arg(), "f");
        assert_eq!(MemoryWordFormat::Character.as_mi_arg(), "c");
        assert_eq!(MemoryWordFormat::String.as_mi_arg(), "s");
        assert_eq!(MemoryWordFormat::Address.as_mi_arg(), "a");
    }

    // ---- Equality and copy semantics for the flag enums ----

    #[test]
    fn print_values_equality() {
        assert_eq!(PrintValues::NoValues, PrintValues::NoValues);
        assert_ne!(PrintValues::NoValues, PrintValues::AllValues);
    }

    #[test]
    fn trace_find_mode_payload_equality() {
        let a = TraceFindMode::PcInsideRange {
            start: "0x400500".into(),
            end: "0x400600".into(),
        };
        let b = TraceFindMode::PcInsideRange {
            start: "0x400500".into(),
            end: "0x400600".into(),
        };
        assert_eq!(a, b);
        let c = TraceFindMode::PcInsideRange {
            start: "0x400500".into(),
            end: "0x400700".into(),
        };
        assert_ne!(a, c);
    }

    #[test]
    fn watch_type_equality() {
        assert_eq!(WatchType::Write, WatchType::Write);
        assert_ne!(WatchType::Read, WatchType::Access);
    }
}
