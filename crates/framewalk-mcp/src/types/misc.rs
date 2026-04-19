use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListThreadGroupsArgs {
    /// List available (unattached) thread groups on the target.
    #[serde(default)]
    pub available: bool,
    /// Recurse into child groups (depth 1).
    #[serde(default)]
    pub recurse: bool,
    /// Specific group ids to list; omit for all.
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InfoOsArgs {
    /// OS info type; omit to list available types.
    #[serde(default)]
    pub info_type: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RemoveInferiorArgs {
    /// Inferior id to remove (must have exited).
    pub inferior_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompleteArgs {
    /// Partial CLI command to complete.
    pub command: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EnableTimingsArgs {
    /// Enable or disable timing collection.
    pub enable: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InfoAdaExceptionsArgs {
    /// Optional regexp to filter exception names.
    #[serde(default)]
    pub regexp: Option<String>,
}
