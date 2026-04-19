use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FilePutArgs {
    pub host_file: String,
    pub target_file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileGetArgs {
    pub target_file: String,
    pub host_file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileDeleteArgs {
    pub target_file: String,
}
