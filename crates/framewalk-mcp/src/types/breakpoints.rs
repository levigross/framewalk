use framewalk_mi_protocol::mi_types::WatchType;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetBreakpointArgs {
    /// Location: function name, `file:line`, or `*address`.
    pub location: String,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub temporary: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BreakpointIdArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetWatchpointArgs {
    pub expression: String,
    #[serde(default = "default_watch_write")]
    pub watch_type: WatchType,
}

fn default_watch_write() -> WatchType {
    WatchType::Write
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BreakConditionArgs {
    pub id: String,
    /// New condition expression, or empty to remove the condition.
    pub condition: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BreakAfterArgs {
    pub id: String,
    /// Ignore the next `count` hits.
    pub count: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BreakCommandsArgs {
    /// Breakpoint number.
    pub id: String,
    /// CLI commands to run when the breakpoint is hit.
    pub commands: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BreakPasscountArgs {
    /// Tracepoint number.
    pub id: String,
    /// Collect data this many times before auto-stopping.
    pub passcount: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DprintfInsertArgs {
    /// Location: function name, `file:line`, or `*address`.
    pub location: String,
    /// Printf format string.
    pub format: String,
    /// Arguments for the format string.
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub ignore_count: Option<u32>,
    #[serde(default)]
    pub thread_id: Option<String>,
}
