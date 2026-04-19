use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetArgsArgs {
    /// Arguments for the inferior on next `-exec-run`.
    pub args: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCwdArgs {
    /// Working directory path.
    pub directory: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetInferiorTtyArgs {
    /// TTY device path for the inferior.
    pub tty: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EnvironmentPathArgs {
    /// Directories to prepend to the executable search path.
    pub directories: Vec<String>,
    /// Reset the path before adding (corresponds to `-r` flag).
    #[serde(default)]
    pub reset: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EnvironmentDirectoryArgs {
    /// Directories to add to the source search path.
    pub directories: Vec<String>,
    /// Reset the path before adding (corresponds to `-r` flag).
    #[serde(default)]
    pub reset: bool,
}
