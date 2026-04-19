//! Feature cache populated from `-list-features` and `-list-target-features`.
//!
//! GDB exposes two capability surfaces: features of the interpreter itself
//! (stable across a session) and features of the currently connected
//! target (which can change across `-target-select` / `-exec-run`). Both
//! are flat lists of opaque strings. framewalk caches them as sorted sets
//! and exposes convenience queries for the capabilities Step 6's MCP tools
//! care about (e.g. `pending-breakpoints`, `async`, `reverse`).

use std::collections::BTreeSet;

use framewalk_mi_codec::{ListValue, Value};

/// Feature flags reported by GDB's `-list-features` /
/// `-list-target-features`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeatureSet {
    features: BTreeSet<String>,
    target_features: BTreeSet<String>,
}

impl FeatureSet {
    /// `true` if GDB reports the given interpreter-level feature.
    #[must_use]
    pub fn has(&self, feature: &str) -> bool {
        self.features.contains(feature)
    }

    /// `true` if GDB reports the given target-level feature.
    #[must_use]
    pub fn target_has(&self, feature: &str) -> bool {
        self.target_features.contains(feature)
    }

    /// All interpreter features in lexicographic order.
    pub fn features(&self) -> impl Iterator<Item = &str> {
        self.features.iter().map(String::as_str)
    }

    /// All target features in lexicographic order.
    pub fn target_features(&self) -> impl Iterator<Item = &str> {
        self.target_features.iter().map(String::as_str)
    }

    // ---- Mutators driven by Connection result routing ----

    /// Handle a `^done,features=[...]` result from `-list-features`.
    pub(crate) fn on_list_features(&mut self, results: &[(String, Value)]) {
        self.features = extract_string_list(results, "features");
    }

    /// Handle a `^done,features=[...]` result from `-list-target-features`.
    /// GDB uses the same `features` key for both commands; the caller
    /// disambiguates based on which command was submitted.
    pub(crate) fn on_list_target_features(&mut self, results: &[(String, Value)]) {
        self.target_features = extract_string_list(results, "features");
    }

    /// Clear the target-features set. Target features can change across
    /// `-target-select` / `-exec-run`, so the connection invalidates them
    /// and re-queries after any of those commands.
    pub(crate) fn invalidate_target_features(&mut self) {
        self.target_features.clear();
    }
}

/// Extract a `[ "a", "b", "c" ]` list of const strings from a result set.
fn extract_string_list(results: &[(String, Value)], key: &str) -> BTreeSet<String> {
    let Some(Value::List(list)) = results
        .iter()
        .find_map(|(k, v)| if k == key { Some(v) } else { None })
    else {
        return BTreeSet::new();
    };
    match list {
        ListValue::Values(vs) => vs
            .iter()
            .filter_map(|v| match v {
                Value::Const(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        ListValue::Results(_) | ListValue::Empty => BTreeSet::new(),
    }
}
