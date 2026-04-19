//! Declarative tool catalog slices used to compose exposure profiles.
//!
//! framewalk stays MI-first: the profile mechanism changes *which* tools
//! are advertised, not what any individual tool means. `full` exposes
//! everything, `core` exposes the common subset plus the `mi_raw_command`
//! and `scheme_eval` escape hatches, and `scheme` exposes only
//! `scheme_eval`.

use rmcp::handler::server::router::tool::ToolRouter;

/// The set of semantic tool names that make up the `core` exposure profile.
pub(crate) fn core_tool_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = crate::tools::all_tool_specs()
        .filter(|spec| spec.in_core())
        .map(|spec| spec.name)
        .collect();
    names.push("scheme_eval");
    names
}

/// Filter a router down to the named tools.
pub(crate) fn retain_only<S>(mut router: ToolRouter<S>, allowed: &[&str]) -> ToolRouter<S>
where
    S: Send + Sync + 'static,
{
    let names: Vec<String> = router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();

    for name in names {
        if !allowed.contains(&name.as_str()) {
            router.remove_route(&name);
        }
    }

    router
}

#[cfg(test)]
mod tests {
    #[test]
    fn core_profile_keeps_escape_hatches() {
        let names = super::core_tool_names();
        assert!(names.contains(&"mi_raw_command"));
        assert!(names.contains(&"scheme_eval"));
    }

    #[test]
    fn core_profile_contains_no_duplicates() {
        let mut names = super::core_tool_names();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), super::core_tool_names().len());
    }

    #[test]
    fn core_profile_names_match_core_tool_specs() {
        let mut expected: Vec<&'static str> = crate::tools::all_tool_specs()
            .filter(|spec| spec.in_core())
            .map(|spec| spec.name)
            .collect();
        expected.push("scheme_eval");
        assert_eq!(super::core_tool_names(), expected);
    }
}
