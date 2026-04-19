use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MiRawCommandArgs {
    /// Raw MI command line, without leading token or trailing newline.
    /// Must begin with `-`. Shell-adjacent commands are rejected unless
    /// `--allow-shell` is enabled.
    pub command: String,
}
