use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UntilArgs {
    /// Location to run until; omit for "until next source line".
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JumpArgs {
    /// Location to jump to (required).
    pub location: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReturnArgs {
    /// Optional return value expression.
    #[serde(default)]
    pub expression: Option<String>,
}

/// Arguments for execution commands that support reverse mode.
/// Requires `target record-full` or similar reverse-debugging setup.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReverseStepArgs {
    /// If true, execute in reverse (requires reverse debugging support).
    #[serde(default)]
    pub reverse: bool,
}
