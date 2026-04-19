use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CatchLoadUnloadArgs {
    /// Library name regexp.
    pub regexp: String,
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CatchAdaExceptionArgs {
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub temporary: bool,
    /// Stop only for this Ada exception name.
    #[serde(default)]
    pub exception_name: Option<String>,
    /// Stop only for unhandled exceptions (mutually exclusive with `exception_name`).
    #[serde(default)]
    pub unhandled: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CatchCppExceptionArgs {
    #[serde(default)]
    pub temporary: bool,
    /// Regexp to filter C++ exception types.
    #[serde(default)]
    pub regexp: Option<String>,
}
