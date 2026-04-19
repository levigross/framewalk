//! Parser for async records: `[token] ("*" | "+" | "=") async-class ("," result)*`.
//!
//! The kind (exec / status / notify) is determined by the leading byte and
//! carried by the enclosing [`Record`](crate::ast::Record) variant; this
//! function parses only the class and trailing results.

use alloc::string::String;

use crate::ast::{AsyncClass, AsyncRecord, Token};
use crate::error::{CodecError, CodecErrorKind};
use crate::parse::cursor::Cursor;
use crate::parse::result::parse_trailing_results;

/// Parse an async record. The cursor must be positioned *after* the leading
/// `*`, `+`, or `=` byte (the top-level dispatcher consumes it and uses it
/// to decide which variant of [`Record`](crate::ast::Record) to build).
pub(crate) fn parse_async_record(
    cursor: &mut Cursor<'_>,
    token: Option<Token>,
) -> Result<AsyncRecord, CodecError> {
    let class = parse_async_class(cursor)?;
    let results = parse_trailing_results(cursor)?;
    Ok(AsyncRecord {
        token,
        class,
        results,
    })
}

fn parse_async_class(cursor: &mut Cursor<'_>) -> Result<AsyncClass, CodecError> {
    let start = cursor.pos();
    // Every accepted byte is ASCII per `is_async_class_byte`, so we push
    // it straight into a `String` as a char. No `expect` needed.
    let mut name = String::new();
    while let Some(b) = cursor.peek() {
        if is_async_class_byte(b) {
            name.push(b as char);
            cursor.advance();
        } else {
            break;
        }
    }
    if name.is_empty() {
        return Err(CodecError::new(CodecErrorKind::InvalidIdentifier, start));
    }
    Ok(AsyncClass::new(name))
}

/// Async class bytes are ASCII letters, digits, and hyphen: `stopped`,
/// `thread-created`, `breakpoint-modified`, `tsv-modified`, etc. Underscore
/// does not appear in documented class names but we accept it defensively.
#[inline]
fn is_async_class_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}
