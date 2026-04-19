use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ThreadSelectArgs {
    pub thread_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ThreadInfoArgs {
    /// Omit to list all threads.
    #[serde(default)]
    pub thread_id: Option<String>,
}
