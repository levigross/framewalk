use framewalk_mi_protocol::mi_types::{PrintValues, RegisterFormat, TraceFindMode};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceInsertArgs {
    /// Location for the tracepoint.
    pub location: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceFindArgs {
    pub mode: TraceFindMode,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceDefineVariableArgs {
    /// Name (must start with `$`).
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceSaveArgs {
    pub filename: String,
    /// Save in CTF format (default tfile).
    #[serde(default)]
    pub ctf: bool,
    /// Target performs the save (default local).
    #[serde(default)]
    pub remote: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceFrameCollectedArgs {
    /// How to print trace variable values.
    #[serde(default)]
    pub var_print_values: Option<PrintValues>,
    /// How to print computed expression values.
    #[serde(default)]
    pub comp_print_values: Option<PrintValues>,
    /// Register value format.
    #[serde(default)]
    pub registers_format: Option<RegisterFormat>,
    /// Include memory contents.
    #[serde(default)]
    pub memory_contents: bool,
}
