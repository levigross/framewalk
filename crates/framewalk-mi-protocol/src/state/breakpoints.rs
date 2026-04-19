//! Breakpoint registry with the mi3-shaped data model.

use std::collections::BTreeMap;

use framewalk_mi_codec::{ListValue, Value};

use crate::results_view::{get_bool, get_str, get_u32};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BreakpointId(String);

impl BreakpointId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint {
    pub id: BreakpointId,
    pub kind: Option<String>,
    pub disp: Option<String>,
    pub enabled: bool,
    pub times: Option<u32>,
    pub locations: Vec<BreakpointLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BreakpointLocation {
    pub number: Option<String>,
    pub addr: Option<String>,
    pub func: Option<String>,
    pub file: Option<String>,
    pub fullname: Option<String>,
    pub line: Option<u32>,
    pub enabled: bool,
}

impl BreakpointLocation {
    fn from_results(results: &[(String, Value)]) -> Self {
        Self {
            number: get_str(results, "number").map(str::to_owned),
            addr: get_str(results, "addr").map(str::to_owned),
            func: get_str(results, "func").map(str::to_owned),
            file: get_str(results, "file").map(str::to_owned),
            fullname: get_str(results, "fullname").map(str::to_owned),
            line: get_u32(results, "line"),
            enabled: get_bool(results, "enabled").unwrap_or(true),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BreakpointRegistry {
    by_id: BTreeMap<BreakpointId, Breakpoint>,
}

impl BreakpointRegistry {
    pub fn iter(&self) -> impl Iterator<Item = (&BreakpointId, &Breakpoint)> {
        self.by_id.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &BreakpointId) -> Option<&Breakpoint> {
        self.by_id.get(id)
    }

    pub(crate) fn upsert_from_bkpt_tuple(&mut self, tuple: &[(String, Value)]) {
        let Some(number) = get_str(tuple, "number") else {
            return;
        };
        let id = BreakpointId::new(number);
        let locations = extract_locations(tuple);

        let bp = Breakpoint {
            id: id.clone(),
            kind: get_str(tuple, "type").map(str::to_owned),
            disp: get_str(tuple, "disp").map(str::to_owned),
            enabled: get_bool(tuple, "enabled").unwrap_or(true),
            times: get_u32(tuple, "times"),
            locations: if locations.is_empty() {
                vec![BreakpointLocation::from_results(tuple)]
            } else {
                locations
            },
        };

        self.by_id.insert(id, bp);
    }

    pub(crate) fn on_breakpoint_created(&mut self, results: &[(String, Value)]) {
        if let Some(tuple) = crate::results_view::get_tuple(results, "bkpt") {
            self.upsert_from_bkpt_tuple(tuple);
        }
    }

    pub(crate) fn on_breakpoint_modified(&mut self, results: &[(String, Value)]) {
        if let Some(tuple) = crate::results_view::get_tuple(results, "bkpt") {
            self.upsert_from_bkpt_tuple(tuple);
        }
    }

    pub(crate) fn on_breakpoint_deleted(&mut self, results: &[(String, Value)]) {
        let Some(id) = get_str(results, "id") else {
            return;
        };
        self.by_id.remove(&BreakpointId::new(id));
    }
}

fn extract_locations(tuple: &[(String, Value)]) -> Vec<BreakpointLocation> {
    let Some(Value::List(list)) =
        tuple
            .iter()
            .find_map(|(k, v)| if k == "locations" { Some(v) } else { None })
    else {
        return Vec::new();
    };
    match list {
        ListValue::Values(vs) => vs
            .iter()
            .filter_map(|v| match v {
                Value::Tuple(pairs) => Some(BreakpointLocation::from_results(pairs)),
                _ => None,
            })
            .collect(),
        ListValue::Results(_) | ListValue::Empty => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(key: &str, val: &str) -> (String, Value) {
        (key.to_string(), Value::Const(val.to_string()))
    }

    fn bkpt_tuple(overrides: &[(&str, &str)]) -> Vec<(String, Value)> {
        // `results_view::get_*` helpers walk left-to-right and return the first
        // match, so overrides must replace in place rather than append.
        let defaults = [
            ("number", "1"),
            ("type", "breakpoint"),
            ("disp", "keep"),
            ("enabled", "y"),
            ("addr", "0x400500"),
            ("func", "main"),
            ("file", "hello.c"),
            ("fullname", "/tmp/hello.c"),
            ("line", "3"),
            ("times", "0"),
        ];
        defaults
            .iter()
            .map(|(k, v)| {
                let val = overrides
                    .iter()
                    .find_map(|(ok, ov)| (ok == k).then_some(*ov))
                    .unwrap_or(v);
                c(k, val)
            })
            .collect()
    }

    // ---- Registry basics ----

    #[test]
    fn empty_registry() {
        let r = BreakpointRegistry::default();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert!(r.get(&BreakpointId::new("1")).is_none());
    }

    #[test]
    fn breakpoint_id_roundtrip() {
        let id = BreakpointId::new("1.2");
        assert_eq!(id.as_str(), "1.2");
    }

    // ---- upsert_from_bkpt_tuple ----

    #[test]
    fn upsert_without_number_is_noop() {
        let mut r = BreakpointRegistry::default();
        let tuple = vec![c("type", "breakpoint")];
        r.upsert_from_bkpt_tuple(&tuple);
        assert!(r.is_empty());
    }

    #[test]
    fn upsert_populates_flat_location() {
        let mut r = BreakpointRegistry::default();
        r.upsert_from_bkpt_tuple(&bkpt_tuple(&[]));
        let bp = r.get(&BreakpointId::new("1")).expect("inserted");
        assert_eq!(bp.kind.as_deref(), Some("breakpoint"));
        assert_eq!(bp.disp.as_deref(), Some("keep"));
        assert!(bp.enabled);
        assert_eq!(bp.times, Some(0));
        // No `locations` list → a synthesized single BreakpointLocation from the flat fields.
        assert_eq!(bp.locations.len(), 1);
        assert_eq!(bp.locations[0].addr.as_deref(), Some("0x400500"));
        assert_eq!(bp.locations[0].func.as_deref(), Some("main"));
        assert_eq!(bp.locations[0].file.as_deref(), Some("hello.c"));
        assert_eq!(bp.locations[0].line, Some(3));
        assert!(bp.locations[0].enabled);
    }

    #[test]
    fn upsert_with_locations_list_splits_into_entries() {
        let loc1 = Value::Tuple(vec![
            c("number", "1.1"),
            c("addr", "0x400500"),
            c("enabled", "y"),
        ]);
        let loc2 = Value::Tuple(vec![
            c("number", "1.2"),
            c("addr", "0x400600"),
            c("enabled", "n"),
        ]);
        let tuple = vec![
            c("number", "1"),
            c("type", "breakpoint"),
            c("enabled", "y"),
            (
                "locations".to_string(),
                Value::List(ListValue::Values(vec![loc1, loc2])),
            ),
        ];
        let mut r = BreakpointRegistry::default();
        r.upsert_from_bkpt_tuple(&tuple);
        let bp = r.get(&BreakpointId::new("1")).unwrap();
        assert_eq!(bp.locations.len(), 2);
        assert_eq!(bp.locations[0].number.as_deref(), Some("1.1"));
        assert!(bp.locations[0].enabled);
        assert_eq!(bp.locations[1].number.as_deref(), Some("1.2"));
        assert!(!bp.locations[1].enabled);
    }

    #[test]
    fn upsert_disabled_bkpt_reports_enabled_false() {
        let mut r = BreakpointRegistry::default();
        r.upsert_from_bkpt_tuple(&bkpt_tuple(&[("enabled", "n")]));
        let bp = r.get(&BreakpointId::new("1")).unwrap();
        assert!(!bp.enabled);
    }

    #[test]
    fn upsert_is_idempotent_and_overwrites() {
        let mut r = BreakpointRegistry::default();
        r.upsert_from_bkpt_tuple(&bkpt_tuple(&[("times", "0")]));
        r.upsert_from_bkpt_tuple(&bkpt_tuple(&[("times", "5")]));
        assert_eq!(r.len(), 1);
        assert_eq!(r.get(&BreakpointId::new("1")).unwrap().times, Some(5));
    }

    // ---- Async notifications ----

    #[test]
    fn on_breakpoint_created_extracts_bkpt_tuple() {
        let mut r = BreakpointRegistry::default();
        let results = vec![("bkpt".to_string(), Value::Tuple(bkpt_tuple(&[])))];
        r.on_breakpoint_created(&results);
        assert!(r.get(&BreakpointId::new("1")).is_some());
    }

    #[test]
    fn on_breakpoint_created_missing_bkpt_is_noop() {
        let mut r = BreakpointRegistry::default();
        let results = vec![c("other", "x")];
        r.on_breakpoint_created(&results);
        assert!(r.is_empty());
    }

    #[test]
    fn on_breakpoint_modified_updates_in_place() {
        let mut r = BreakpointRegistry::default();
        let created = vec![(
            "bkpt".to_string(),
            Value::Tuple(bkpt_tuple(&[("times", "0")])),
        )];
        r.on_breakpoint_created(&created);
        let modified = vec![(
            "bkpt".to_string(),
            Value::Tuple(bkpt_tuple(&[("times", "7")])),
        )];
        r.on_breakpoint_modified(&modified);
        assert_eq!(r.get(&BreakpointId::new("1")).unwrap().times, Some(7));
    }

    #[test]
    fn on_breakpoint_deleted_removes_by_id() {
        let mut r = BreakpointRegistry::default();
        r.upsert_from_bkpt_tuple(&bkpt_tuple(&[]));
        assert_eq!(r.len(), 1);
        r.on_breakpoint_deleted(&[c("id", "1")]);
        assert!(r.is_empty());
    }

    #[test]
    fn on_breakpoint_deleted_missing_id_is_noop() {
        let mut r = BreakpointRegistry::default();
        r.upsert_from_bkpt_tuple(&bkpt_tuple(&[]));
        r.on_breakpoint_deleted(&[c("other", "x")]);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn iter_is_ordered_by_breakpoint_id() {
        let mut r = BreakpointRegistry::default();
        let mut with_id = |n: &str| {
            let mut t = bkpt_tuple(&[]);
            // Replace the "number" field — first entry in bkpt_tuple.
            t[0] = c("number", n);
            r.upsert_from_bkpt_tuple(&t);
        };
        with_id("3");
        with_id("1");
        with_id("2");
        let ids: Vec<_> = r.iter().map(|(id, _)| id.as_str().to_owned()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }
}
