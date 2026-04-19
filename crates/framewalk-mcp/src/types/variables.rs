use framewalk_mi_protocol::mi_types::{PrintValues, VarFormat};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectArgs {
    /// Expression to evaluate in the current frame.
    pub expression: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WatchCreateArgs {
    /// Expression to watch.
    pub expression: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WatchDeleteArgs {
    /// Variable-object name.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarNameArgs {
    /// Variable-object name.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarListChildrenArgs {
    /// Variable-object name.
    pub name: String,
    /// How to print child values.
    #[serde(default)]
    pub print_values: Option<PrintValues>,
    /// First child index (0-based).
    #[serde(default)]
    pub from: Option<u32>,
    /// Last child index (exclusive).
    #[serde(default)]
    pub to: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarAssignArgs {
    /// Variable-object name.
    pub name: String,
    /// New value expression.
    pub expression: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarSetFormatArgs {
    /// Variable-object name.
    pub name: String,
    /// Display format.
    pub format: VarFormat,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarSetFrozenArgs {
    /// Variable-object name.
    pub name: String,
    /// True to freeze (stop updates), false to thaw.
    pub frozen: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarSetUpdateRangeArgs {
    /// Variable-object name.
    pub name: String,
    /// First child index to update.
    pub from: u32,
    /// Last child index to update (exclusive).
    pub to: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VarSetVisualizerArgs {
    /// Variable-object name.
    pub name: String,
    /// Visualizer expression (Python visualizer name, or `"None"` to reset).
    pub visualizer: String,
}
