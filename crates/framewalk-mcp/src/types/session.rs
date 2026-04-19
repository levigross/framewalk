use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DrainEventsArgs {
    /// Return events strictly after this cursor. Omit or pass `0` to read
    /// the full retained journal window.
    #[serde(default)]
    pub cursor: Option<u64>,
}
