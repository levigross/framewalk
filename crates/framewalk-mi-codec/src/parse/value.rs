//! Parser for MI `value` productions: `const | tuple | list`.
//!
//! Corresponds directly to the BNF:
//!
//! ```text
//! result → variable "=" value
//! value  → const | tuple | list
//! const  → c-string
//! tuple  → "{}" | "{" result ("," result)* "}"
//! list   → "[]"
//!        | "[" value ("," value)* "]"
//!        | "[" result ("," result)* "]"
//! ```
//!
//! Dispatch is one-byte LL(1): a value starting with `"` is a const, `{` is
//! a tuple, `[` is a list. Lists further disambiguate between a
//! [`ListValue::Values`] and a [`ListValue::Results`] on the first element:
//! if the first item is a bare `value`, every subsequent item must also be
//! a bare value; if it's a `result` (name=value), every subsequent item
//! must also be a result. Mixed lists are a
//! [`CodecErrorKind::MixedList`](crate::error::CodecErrorKind::MixedList)
//! error.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::{ListValue, Value};
use crate::error::{CodecError, CodecErrorKind, Expected};
use crate::parse::cstring::parse_cstring;
use crate::parse::cursor::Cursor;

/// Parse a `value` at the cursor.
pub(crate) fn parse_value(cursor: &mut Cursor<'_>) -> Result<Value, CodecError> {
    match cursor.peek() {
        Some(b'"') => parse_cstring(cursor).map(Value::Const),
        Some(b'{') => parse_tuple(cursor).map(Value::Tuple),
        Some(b'[') => parse_list(cursor).map(Value::List),
        Some(found) => Err(CodecError::new(
            CodecErrorKind::UnexpectedByte {
                expected: Expected::OneOf(b"\"{["),
                found,
            },
            cursor.pos(),
        )),
        None => Err(CodecError::new(CodecErrorKind::UnexpectedEnd, cursor.pos())),
    }
}

/// Parse a `tuple`: `"{}" | "{" result ("," result)* "}"`.
pub(crate) fn parse_tuple(cursor: &mut Cursor<'_>) -> Result<Vec<(String, Value)>, CodecError> {
    cursor.expect_byte(b'{')?;
    let mut out: Vec<(String, Value)> = Vec::new();

    if cursor.peek() == Some(b'}') {
        cursor.advance();
        return Ok(out);
    }

    loop {
        let (name, value) = parse_result(cursor)?;
        out.push((name, value));
        match cursor.peek() {
            Some(b',') => {
                cursor.advance();
            }
            Some(b'}') => {
                cursor.advance();
                return Ok(out);
            }
            Some(found) => {
                return Err(CodecError::new(
                    CodecErrorKind::UnexpectedByte {
                        expected: Expected::OneOf(b",}"),
                        found,
                    },
                    cursor.pos(),
                ));
            }
            None => {
                return Err(CodecError::new(CodecErrorKind::UnexpectedEnd, cursor.pos()));
            }
        }
    }
}

/// Parse a `list`.
///
/// The three productions (`[]`, `[value,...]`, `[result,...]`) are
/// distinguished by looking at the first element: if it begins with an
/// identifier byte followed by `=`, it's a results list; otherwise it's a
/// values list.
pub(crate) fn parse_list(cursor: &mut Cursor<'_>) -> Result<ListValue, CodecError> {
    cursor.expect_byte(b'[')?;

    if cursor.peek() == Some(b']') {
        cursor.advance();
        return Ok(ListValue::Empty);
    }

    // Look one byte ahead past the first identifier-like token (if any) to
    // decide whether this is a list-of-results or a list-of-values.
    if looks_like_result(cursor) {
        let mut out: Vec<(String, Value)> = Vec::new();
        loop {
            let (name, value) = parse_result(cursor)?;
            out.push((name, value));
            match cursor.peek() {
                Some(b',') => {
                    cursor.advance();
                    // Enforce homogeneity: after a result, next must also
                    // be a result.
                    if !looks_like_result(cursor) {
                        return Err(CodecError::new(CodecErrorKind::MixedList, cursor.pos()));
                    }
                }
                Some(b']') => {
                    cursor.advance();
                    return Ok(ListValue::Results(out));
                }
                Some(found) => {
                    return Err(CodecError::new(
                        CodecErrorKind::UnexpectedByte {
                            expected: Expected::OneOf(b",]"),
                            found,
                        },
                        cursor.pos(),
                    ));
                }
                None => {
                    return Err(CodecError::new(CodecErrorKind::UnexpectedEnd, cursor.pos()));
                }
            }
        }
    } else {
        let mut out: Vec<Value> = Vec::new();
        loop {
            let value = parse_value(cursor)?;
            out.push(value);
            match cursor.peek() {
                Some(b',') => {
                    cursor.advance();
                    if looks_like_result(cursor) {
                        return Err(CodecError::new(CodecErrorKind::MixedList, cursor.pos()));
                    }
                }
                Some(b']') => {
                    cursor.advance();
                    return Ok(ListValue::Values(out));
                }
                Some(found) => {
                    return Err(CodecError::new(
                        CodecErrorKind::UnexpectedByte {
                            expected: Expected::OneOf(b",]"),
                            found,
                        },
                        cursor.pos(),
                    ));
                }
                None => {
                    return Err(CodecError::new(CodecErrorKind::UnexpectedEnd, cursor.pos()));
                }
            }
        }
    }
}

/// Parse a `result`: `variable "=" value`. `variable` is a non-empty
/// identifier matching the alphabet `[A-Za-z0-9_-]` (GDB uses hyphens in
/// result names like `thread-id` and `bkpt-no`).
pub(crate) fn parse_result(cursor: &mut Cursor<'_>) -> Result<(String, Value), CodecError> {
    let name = parse_variable(cursor)?;
    cursor.expect_byte(b'=').map_err(|e| {
        // Remap a generic "expected '=' got X" into a more specific error
        // so callers see `ExpectedEquals` rather than a byte mismatch.
        match e.kind {
            CodecErrorKind::UnexpectedByte { .. } => {
                CodecError::new(CodecErrorKind::ExpectedEquals, e.offset)
            }
            _ => e,
        }
    })?;
    let value = parse_value(cursor)?;
    Ok((name, value))
}

/// Parse a `variable`: a non-empty run of identifier bytes.
///
/// The GDB manual does not tightly specify the identifier alphabet for
/// result names; in practice GDB uses ASCII letters, digits, underscore,
/// and hyphen. framewalk accepts that set and rejects anything else.
///
/// Each accepted byte is ASCII by the `is_identifier_byte` check, so we
/// can push it directly as a char without a subsequent UTF-8 validation
/// step. No `expect` or `unwrap`: by construction this function cannot
/// panic on any input.
fn parse_variable(cursor: &mut Cursor<'_>) -> Result<String, CodecError> {
    let start_pos = cursor.pos();
    let mut name = String::new();
    while let Some(b) = cursor.peek() {
        if is_identifier_byte(b) {
            name.push(b as char); // b is ASCII by is_identifier_byte
            cursor.advance();
        } else {
            break;
        }
    }
    if name.is_empty() {
        return Err(CodecError::new(
            CodecErrorKind::InvalidIdentifier,
            start_pos,
        ));
    }
    Ok(name)
}

/// Return `true` if the current cursor position looks like the start of a
/// `result`: an identifier byte that is followed (after zero or more
/// identifier bytes) by `=`.
fn looks_like_result(cursor: &Cursor<'_>) -> bool {
    let Some(first) = cursor.peek() else {
        return false;
    };
    if !is_identifier_byte(first) {
        return false;
    }
    // Scan ahead byte-by-byte until we see a non-identifier byte. If that
    // byte is `=`, it's a result; otherwise (e.g. `,`, `]`) it's not.
    let mut offset = 1;
    while let Some(b) = cursor.peek_at(offset) {
        if is_identifier_byte(b) {
            offset += 1;
        } else {
            return b == b'=';
        }
    }
    false
}

#[inline]
fn is_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}
