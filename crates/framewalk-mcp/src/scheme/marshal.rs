//! Marshalling between GDB/MI protocol types and Steel Scheme values.
//!
//! The core translation is [`outcome_to_steel`] which turns a
//! [`CommandOutcome`] into a [`SteelVal`] that Scheme code can
//! destructure with standard hash-map and list operations.
//!
//! ## Representation mapping
//!
//! | MI type                      | Scheme value                        |
//! |------------------------------|-------------------------------------|
//! | `CommandOutcome::Done`       | result-entry list                   |
//! | `CommandOutcome::Running`    | symbol `'running`                   |
//! | `CommandOutcome::Connected`  | result-entry list (same shape)      |
//! | `CommandOutcome::Error`      | *not represented* — raised as error |
//! | `CommandOutcome::Exit`       | *not represented* — raised as error |
//! | `Value::Const(s)`            | string                              |
//! | `Value::Tuple(pairs)`        | result-entry list                   |
//! | `Value::List(Empty)`         | `'()`                               |
//! | `Value::List(Values(vs))`    | list `(v₁ v₂ …)`                   |
//! | `Value::List(Results(pairs))`| result-entry list                   |
//!
//! A result-entry list is an ordered Scheme list of tiny hash-maps with
//! `"name"` and `"value"` keys, matching the JSON tool-result shape.
//! This is the lossless representation: MI preserves key order and
//! allows duplicate names, so flattening tuples into a hash-map is
//! semantically wrong for commands like `-stack-list-frames`.

use framewalk_mi_codec::{ListValue, Value};
use framewalk_mi_protocol::CommandOutcome;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use steel::gc::Gc;
use steel::rerrs::{ErrorKind, SteelErr};
use steel::rvals::{SteelHashMap, SteelString, SteelVal};
use steel::HashMap;

/// Maximum byte length for fallback stringification of a single Scheme
/// value when it cannot be represented cleanly as JSON. Prevents an
/// unsupported runtime value from consuming unbounded context-window
/// tokens when it is surfaced through `scheme_eval`.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

// ---------------------------------------------------------------------------
// CommandOutcome → SteelVal
// ---------------------------------------------------------------------------

/// Convert a [`CommandOutcome`] into a [`SteelVal`].
///
/// `Done` and `Connected` yield lossless result-entry lists.
/// `Running` yields the symbol `running`.
/// `Error` and `Exit` return a [`SteelErr`] so the calling Scheme code
/// sees a raised exception rather than a normal return value.
pub(crate) fn outcome_to_steel(outcome: &CommandOutcome) -> Result<SteelVal, SteelErr> {
    match outcome {
        CommandOutcome::Done(results) | CommandOutcome::Connected(results) => {
            Ok(results_to_steel(results))
        }
        CommandOutcome::Running => Ok(SteelVal::SymbolV(SteelString::from("running"))),
        CommandOutcome::Error { msg, .. } => Err(SteelErr::new(
            ErrorKind::Generic,
            format!("GDB error: {msg}"),
        )),
        CommandOutcome::Exit => Err(SteelErr::new(
            ErrorKind::Generic,
            "GDB session exited".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// MI Value → SteelVal
// ---------------------------------------------------------------------------

/// Convert a single MI [`Value`] into a [`SteelVal`].
pub(crate) fn value_to_steel(value: &Value) -> SteelVal {
    match value {
        Value::Const(s) => SteelVal::StringV(SteelString::from(s.as_str())),
        Value::Tuple(pairs) => results_to_steel(pairs),
        Value::List(list_val) => match list_val {
            ListValue::Empty => SteelVal::ListV(Vec::new().into()),
            ListValue::Values(vs) => {
                let items: Vec<SteelVal> = vs.iter().map(value_to_steel).collect();
                SteelVal::ListV(items.into())
            }
            ListValue::Results(pairs) => results_to_steel(pairs),
        },
    }
}

/// Convert a result tuple `Vec<(String, Value)>` into a lossless Scheme
/// list of `{name, value}` entry hash-maps.
fn results_to_steel(results: &[(String, Value)]) -> SteelVal {
    let entries: Vec<SteelVal> = results
        .iter()
        .map(|(key, value)| result_entry_to_steel(key, value))
        .collect();
    SteelVal::ListV(entries.into())
}

fn result_entry_to_steel(key: &str, value: &Value) -> SteelVal {
    let mut map = HashMap::new();
    map.insert(
        SteelVal::StringV(SteelString::from("name")),
        SteelVal::StringV(SteelString::from(key)),
    );
    map.insert(
        SteelVal::StringV(SteelString::from("value")),
        value_to_steel(value),
    );
    SteelVal::HashMapV(SteelHashMap::from(Gc::new(map)))
}

// ---------------------------------------------------------------------------
// SteelVal → JSON / String (for MCP response)
// ---------------------------------------------------------------------------

/// Convert one Scheme value into the structured JSON representation
/// exposed by `scheme_eval`.
///
/// Supported first-class data becomes native JSON where possible:
/// booleans, numbers, strings, chars, symbols, lists/vectors, `void`,
/// and string-key hash maps. Runtime-only values (functions, ports,
/// iterators, non-string-key hash maps, etc.) fall back to their compact
/// display string so the result remains serialisable and bounded.
pub(crate) fn steel_to_json_value(value: &SteelVal) -> JsonValue {
    match value {
        SteelVal::BoolV(flag) => JsonValue::Bool(*flag),
        SteelVal::NumV(num) => JsonNumber::from_f64(*num)
            .map_or_else(|| fallback_json_string(value), JsonValue::Number),
        SteelVal::IntV(num) => JsonValue::Number(i64::try_from(*num).unwrap_or(i64::MAX).into()),
        SteelVal::StringV(text) | SteelVal::SymbolV(text) => JsonValue::String(text.to_string()),
        SteelVal::CharV(ch) => JsonValue::String(ch.to_string()),
        SteelVal::Void => JsonValue::Null,
        SteelVal::ListV(items) => JsonValue::Array(items.iter().map(steel_to_json_value).collect()),
        SteelVal::VectorV(items) => {
            JsonValue::Array(items.iter().map(steel_to_json_value).collect())
        }
        SteelVal::HashMapV(map) => {
            steel_hash_map_to_json(map).unwrap_or_else(|| fallback_json_string(value))
        }
        _ => fallback_json_string(value),
    }
}

/// Convert the full set of top-level expression results from one
/// `engine.run(...)` call into the public `scheme_eval.result` JSON
/// value. A single top-level expression serialises directly; multiple
/// expressions become an array preserving evaluation order.
pub(crate) fn steel_values_to_json(values: &[SteelVal]) -> JsonValue {
    match values {
        [] => JsonValue::Null,
        [single] => steel_to_json_value(single),
        many => JsonValue::Array(many.iter().map(steel_to_json_value).collect()),
    }
}

fn steel_hash_map_to_json(map: &SteelHashMap) -> Option<JsonValue> {
    let mut obj = JsonMap::new();
    for (key, value) in map.iter() {
        let key = steel_json_object_key(key)?;
        obj.insert(key, steel_to_json_value(value));
    }
    Some(JsonValue::Object(obj))
}

fn steel_json_object_key(key: &SteelVal) -> Option<String> {
    match key {
        SteelVal::StringV(text) | SteelVal::SymbolV(text) => Some(text.to_string()),
        SteelVal::CharV(ch) => Some(ch.to_string()),
        _ => None,
    }
}

fn fallback_json_string(value: &SteelVal) -> JsonValue {
    JsonValue::String(truncate_display_string(format!("{value}")))
}

fn truncate_display_string(mut rendered: String) -> String {
    if rendered.len() > MAX_OUTPUT_BYTES {
        rendered.truncate(MAX_OUTPUT_BYTES);
        rendered.push_str("\n…[truncated]");
    }
    rendered
}

/// Serialise the results of a Scheme evaluation into a string suitable
/// for an MCP `Content::text` block.
///
/// Multiple top-level values (one per expression) are separated by
/// newlines.  Output is truncated to [`MAX_OUTPUT_BYTES`] with an
/// appended marker.
pub(crate) fn steel_to_display_string(values: &[SteelVal]) -> String {
    let mut buf = String::new();
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        let formatted = truncate_display_string(format!("{val}"));
        buf.push_str(&formatted);
        if buf.len() > MAX_OUTPUT_BYTES {
            buf.truncate(MAX_OUTPUT_BYTES);
            buf.push_str("\n…[truncated]");
            break;
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn string_value(text: &str) -> SteelVal {
        SteelVal::StringV(SteelString::from(text))
    }

    fn entry_field<'a>(entry: &'a SteelVal, name: &str) -> &'a SteelVal {
        let SteelVal::HashMapV(map) = entry else {
            panic!("expected entry hash-map, got {entry:?}");
        };
        map.get(&string_value(name))
            .unwrap_or_else(|| panic!("missing field {name} in {entry:?}"))
    }

    #[test]
    fn const_becomes_string() {
        let val = value_to_steel(&Value::Const("hello".to_string()));
        assert!(matches!(val, SteelVal::StringV(_)));
    }

    #[test]
    fn empty_list_becomes_empty_steel_list() {
        let val = value_to_steel(&Value::List(ListValue::Empty));
        assert!(matches!(val, SteelVal::ListV(_)));
    }

    #[test]
    fn done_outcome_becomes_entry_list() {
        let outcome = CommandOutcome::Done(vec![
            ("a".to_string(), Value::Const("1".to_string())),
            ("b".to_string(), Value::Const("2".to_string())),
        ]);
        let steel = outcome_to_steel(&outcome).expect("should convert");
        let SteelVal::ListV(entries) = steel else {
            panic!("expected list, got {steel:?}");
        };
        assert_eq!(entries.len(), 2);
        assert_eq!(entry_field(&entries[0], "name"), &string_value("a"));
        assert_eq!(entry_field(&entries[1], "name"), &string_value("b"));
    }

    #[test]
    fn duplicate_names_are_preserved_in_order() {
        let steel = value_to_steel(&Value::Tuple(vec![
            ("frame".to_string(), Value::Const("0".to_string())),
            ("frame".to_string(), Value::Const("1".to_string())),
        ]));
        let SteelVal::ListV(entries) = steel else {
            panic!("expected list, got {steel:?}");
        };
        assert_eq!(entries.len(), 2);
        assert_eq!(entry_field(&entries[0], "name"), &string_value("frame"));
        assert_eq!(entry_field(&entries[0], "value"), &string_value("0"));
        assert_eq!(entry_field(&entries[1], "name"), &string_value("frame"));
        assert_eq!(entry_field(&entries[1], "value"), &string_value("1"));
    }

    #[test]
    fn running_outcome_becomes_symbol() {
        let steel = outcome_to_steel(&CommandOutcome::Running).expect("should convert");
        assert!(matches!(steel, SteelVal::SymbolV(_)));
    }

    #[test]
    fn error_outcome_raises_steel_error() {
        let outcome = CommandOutcome::Error {
            msg: "oops".to_string(),
            code: None,
        };
        let err = outcome_to_steel(&outcome).expect_err("should be an error");
        assert!(err.to_string().contains("oops"));
    }

    #[test]
    fn exit_outcome_raises_steel_error() {
        let err = outcome_to_steel(&CommandOutcome::Exit).expect_err("should be an error");
        assert!(err.to_string().contains("exited"));
    }

    #[test]
    fn truncation_works() {
        let big = SteelVal::StringV(SteelString::from(
            "x".repeat(MAX_OUTPUT_BYTES + 100).as_str(),
        ));
        let out = steel_to_display_string(&[big]);
        assert!(out.len() <= MAX_OUTPUT_BYTES + 20);
        assert!(out.contains("truncated"));
    }

    #[test]
    fn top_level_int_becomes_json_number() {
        let json = steel_values_to_json(&[SteelVal::IntV(42)]);
        assert_eq!(json, serde_json::json!(42));
    }

    #[test]
    fn multiple_top_level_values_become_json_array() {
        let json = steel_values_to_json(&[
            SteelVal::IntV(1),
            SteelVal::StringV(SteelString::from("two")),
        ]);
        assert_eq!(json, serde_json::json!([1, "two"]));
    }

    #[test]
    fn string_key_hash_map_becomes_json_object() {
        let mut map = HashMap::new();
        map.insert(string_value("alpha"), SteelVal::IntV(1));
        map.insert(string_value("beta"), SteelVal::BoolV(true));

        let json = steel_to_json_value(&SteelVal::HashMapV(SteelHashMap::from(Gc::new(map))));
        assert_eq!(json, serde_json::json!({"alpha": 1, "beta": true}));
    }

    #[test]
    fn entry_lists_remain_json_arrays_of_name_value_objects() {
        let steel = value_to_steel(&Value::Tuple(vec![
            ("frame".to_string(), Value::Const("0".to_string())),
            ("frame".to_string(), Value::Const("1".to_string())),
        ]));
        let json = steel_to_json_value(&steel);
        assert_eq!(
            json,
            serde_json::json!([
                {"name": "frame", "value": "0"},
                {"name": "frame", "value": "1"}
            ])
        );
    }

    #[test]
    fn non_string_key_hash_map_falls_back_to_string() {
        let mut map = HashMap::new();
        map.insert(
            SteelVal::IntV(7),
            SteelVal::StringV(SteelString::from("seven")),
        );
        let json = steel_to_json_value(&SteelVal::HashMapV(SteelHashMap::from(Gc::new(map))));
        let JsonValue::String(rendered) = json else {
            panic!("expected fallback string");
        };
        assert!(rendered.contains('7'));
        assert!(rendered.contains("seven"));
    }
}
