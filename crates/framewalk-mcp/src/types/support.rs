use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GdbSetArgs {
    /// GDB variable assignment (e.g. `"pagination off"`).
    pub variable: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GdbShowArgs {
    /// GDB variable name to query.
    pub variable: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InfoMiCommandArgs {
    /// MI command name to query (with or without leading `-`).
    pub command: String,
}
