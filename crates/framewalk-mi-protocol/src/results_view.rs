//! Borrowing accessors for MI result tuples.
//!
//! MI results are `Vec<(String, Value)>` — ordered key-value pairs where
//! keys may repeat. Every state module needs to pull fields out of these
//! tuples. This module provides a single set of borrowing helpers so the
//! lookup pattern isn't duplicated across breakpoints, threads, frames,
//! varobjs, and connection.
//!
//! All accessors return borrowed references (&str, &[(String, Value)])
//! and never clone, so callers that only need to inspect a field pay zero
//! allocation. Callers that need to own the value call `.to_owned()` or
//! `.to_string()` at the call site, making the allocation explicit.

use framewalk_mi_codec::Value;

/// Look up a `Value::Const` field by name, returning a borrowed `&str`.
pub(crate) fn get_str<'a>(results: &'a [(String, Value)], name: &str) -> Option<&'a str> {
    results.iter().find_map(|(k, v)| match v {
        Value::Const(s) if k == name => Some(s.as_str()),
        _ => None,
    })
}

/// Look up a `Value::Const` field by name and return an owned clone.
/// Use when you need to store the value; prefer [`get_str`] when you
/// only need to inspect it.
pub(crate) fn get_string(results: &[(String, Value)], name: &str) -> Option<String> {
    get_str(results, name).map(str::to_owned)
}

/// Look up a `Value::Tuple` field by name, returning a borrowed slice
/// of the tuple's key-value pairs.
pub(crate) fn get_tuple<'a>(
    results: &'a [(String, Value)],
    name: &str,
) -> Option<&'a [(String, Value)]> {
    results.iter().find_map(|(k, v)| match v {
        Value::Tuple(pairs) if k == name => Some(pairs.as_slice()),
        _ => None,
    })
}

/// Look up a `Value::Const` field and parse it as a `bool`.
/// GDB uses `"y"` / `"n"` for boolean flags.
pub(crate) fn get_bool(results: &[(String, Value)], name: &str) -> Option<bool> {
    get_str(results, name).map(|s| s == "y" || s == "yes" || s == "true")
}

/// Look up a `Value::Const` field and parse it as a `u32`.
pub(crate) fn get_u32(results: &[(String, Value)], name: &str) -> Option<u32> {
    get_str(results, name).and_then(|s| s.parse().ok())
}

/// Look up a `Value::Const` field and parse it as an `i32`.
pub(crate) fn get_i32(results: &[(String, Value)], name: &str) -> Option<i32> {
    get_str(results, name).and_then(|s| s.parse().ok())
}
