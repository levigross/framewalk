use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AttachArgs {
    /// OS process id to attach to.
    pub pid: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TargetSelectArgs {
    /// Transport type: `"remote"`, `"extended-remote"`, `"sim"`, etc.
    pub transport: String,
    /// Transport parameters (e.g. `"localhost:3333"`).
    pub parameters: String,
}
