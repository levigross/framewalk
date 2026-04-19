use framewalk_mi_protocol::mi_types::{MemoryWordFormat, OpcodeMode, RegisterFormat};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadMemoryArgs {
    /// Start address (hex or expression).
    pub address: String,
    /// Number of addressable memory units to read.
    pub count: u64,
    /// Offset relative to address.
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteMemoryArgs {
    /// Start address (hex or expression).
    pub address: String,
    /// Hex-encoded byte contents.
    pub contents: String,
    /// Byte count (if > contents length, GDB repeats the pattern).
    #[serde(default)]
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DisassembleArgs {
    /// Start address (hex string).
    pub start_addr: String,
    /// End address (hex string).
    pub end_addr: String,
    /// Opcodes display mode.
    #[serde(default)]
    pub opcodes: Option<OpcodeMode>,
    /// Include interleaved source lines.
    #[serde(default)]
    pub source: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RegisterValuesArgs {
    /// Value format.
    pub format: RegisterFormat,
    /// Specific register numbers; omit for all.
    #[serde(default)]
    pub registers: Vec<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RegisterNamesArgs {
    /// Specific register numbers; omit for all.
    #[serde(default)]
    pub registers: Vec<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadMemoryDeprecatedArgs {
    /// Start address (hex or expression).
    pub address: String,
    /// Word format.
    pub word_format: MemoryWordFormat,
    /// Word size in bytes (1, 2, 4, or 8).
    pub word_size: u32,
    /// Number of rows.
    pub nr_rows: u32,
    /// Number of columns per row.
    pub nr_cols: u32,
    /// Column offset (in word-size units).
    #[serde(default)]
    pub column_offset: Option<i64>,
    /// Character to display for non-printable bytes.
    #[serde(default)]
    pub aschar: Option<String>,
}
