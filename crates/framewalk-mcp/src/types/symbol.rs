use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolInfoArgs {
    /// Filter by name regexp.
    #[serde(default)]
    pub name: Option<String>,
    /// Filter by type regexp (functions and variables only).
    #[serde(default)]
    pub type_regexp: Option<String>,
    /// Include non-debug symbols (functions and variables only).
    #[serde(default)]
    pub include_nondebug: bool,
    /// Maximum number of results.
    #[serde(default)]
    pub max_results: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolModuleArgs {
    /// Filter by module name regexp.
    #[serde(default)]
    pub module: Option<String>,
    /// Filter by symbol name regexp.
    #[serde(default)]
    pub name: Option<String>,
    /// Filter by type regexp.
    #[serde(default)]
    pub type_regexp: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolListLinesArgs {
    /// Source file name.
    pub filename: String,
}
