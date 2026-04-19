use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FilePathArgs {
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSourceFilesArgs {
    /// Group results by object file.
    #[serde(default)]
    pub group_by_objfile: bool,
    /// Filter source files by regexp.
    #[serde(default)]
    pub regexp: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSharedLibrariesArgs {
    /// Filter libraries by regexp.
    #[serde(default)]
    pub regexp: Option<String>,
}
