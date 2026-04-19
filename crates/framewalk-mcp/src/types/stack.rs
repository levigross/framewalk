use framewalk_mi_protocol::mi_types::PrintValues;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SelectFrameArgs {
    /// Frame level (`0` = innermost).
    pub level: u32,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct BacktraceArgs {
    /// Return at most this many frames (innermost first). Omit for the
    /// full stack. Passed to `-stack-list-frames` as an inclusive
    /// `0..=limit-1` range.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StackListArgs {
    /// How to print variable values.
    pub print_values: PrintValues,
    #[serde(default)]
    pub skip_unavailable: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StackListArgumentsArgs {
    pub print_values: PrintValues,
    #[serde(default)]
    pub skip_unavailable: bool,
    /// Low frame level (inclusive); omit for all frames.
    #[serde(default)]
    pub low_frame: Option<u32>,
    /// High frame level (inclusive); omit for all frames.
    #[serde(default)]
    pub high_frame: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StackDepthArgs {
    /// Maximum depth to probe; omit for unlimited.
    #[serde(default)]
    pub max_depth: Option<u32>,
}
