//! Parser for result records: `[token] "^" result-class ("," result)*`.

use alloc::vec::Vec;

use crate::ast::{ResultClass, ResultRecord, Token, Value};
use alloc::string::String;

use crate::error::{CodecError, CodecErrorKind};
use crate::parse::cursor::Cursor;
use crate::parse::value::parse_result;

/// Parse a result record. The cursor must be positioned *after* the leading
/// `^` byte (the top-level dispatcher consumes it before delegating here).
/// The caller also supplies the already-parsed optional token.
pub(crate) fn parse_result_record(
    cursor: &mut Cursor<'_>,
    token: Option<Token>,
) -> Result<ResultRecord, CodecError> {
    let class = parse_result_class(cursor)?;
    let results = parse_trailing_results(cursor)?;
    Ok(ResultRecord {
        token,
        class,
        results,
    })
}

fn parse_result_class(cursor: &mut Cursor<'_>) -> Result<ResultClass, CodecError> {
    let start = cursor.pos();
    // Accumulate into a String directly; every byte we push has been
    // checked as `is_ascii_lowercase`, so the `b as char` cast is
    // lossless and cannot produce invalid UTF-8.
    let mut name = String::new();
    while let Some(b) = cursor.peek() {
        if b.is_ascii_lowercase() {
            name.push(b as char);
            cursor.advance();
        } else {
            break;
        }
    }
    if name.is_empty() {
        return Err(CodecError::new(CodecErrorKind::InvalidIdentifier, start));
    }
    ResultClass::from_bytes(name.as_bytes()).ok_or(CodecError::new(
        CodecErrorKind::UnknownResultClass { name },
        start,
    ))
}

/// Parse a trailing sequence of `("," result)*` up to end of input.
///
/// Shared by result records and async records: after the class token, both
/// productions are a possibly-empty comma-separated list of `name=value`
/// results running to the end of the line.
pub(crate) fn parse_trailing_results(
    cursor: &mut Cursor<'_>,
) -> Result<Vec<(String, Value)>, CodecError> {
    let mut out: Vec<(String, Value)> = Vec::new();
    loop {
        match cursor.peek() {
            None => return Ok(out),
            Some(b',') => {
                cursor.advance();
                let (name, value) = parse_result(cursor)?;
                out.push((name, value));
            }
            Some(found) => {
                return Err(CodecError::new(
                    CodecErrorKind::UnexpectedByte {
                        expected: crate::error::Expected::Description("',' or end of record"),
                        found,
                    },
                    cursor.pos(),
                ));
            }
        }
    }
}
