//! Recursive-descent parser for the GDB/MI v3 output grammar.
//!
//! Entry point: [`parse_record`], which consumes a single complete MI line
//! (as produced by [`framewalk_mi_wire::Framer`](framewalk_mi_wire::Framer))
//! and returns a typed [`Record`](crate::ast::Record) or a
//! [`CodecError`](crate::error::CodecError).

pub(crate) mod async_rec;
pub(crate) mod cstring;
pub(crate) mod cursor;
pub(crate) mod result;
pub(crate) mod stream;
pub(crate) mod value;

use crate::ast::{Record, Token};
use crate::error::{CodecError, CodecErrorKind};
use crate::parse::async_rec::parse_async_record;
use crate::parse::cursor::Cursor;
use crate::parse::result::parse_result_record;
use crate::parse::stream::parse_stream_record;

/// Parse a single MI output record from a line of bytes.
///
/// `line` is one complete MI output line with the trailing `\n` (or
/// `\r\n`) already stripped. This is exactly what
/// [`framewalk_mi_wire::Framer::pop`](framewalk_mi_wire::Framer::pop)
/// produces in a [`Frame::Line`](framewalk_mi_wire::Frame::Line) variant.
/// Do not pass the literal `(gdb)` terminator line — the wire framer yields
/// it as [`Frame::GroupTerminator`](framewalk_mi_wire::Frame::GroupTerminator),
/// and it has no representation in the output grammar.
///
/// # Errors
///
/// Returns [`CodecError`] with a precise byte offset into `line` if the
/// input does not conform to the MI output grammar, if a c-string contains
/// an invalid escape, or if decoded c-string contents are not valid UTF-8.
pub fn parse_record(line: &[u8]) -> Result<Record, CodecError> {
    let mut cursor = Cursor::new(line);

    // 1. Optional leading token (digit+). Stream-record prefixes forbid
    //    a token, which we check below.
    let token = parse_optional_token(&mut cursor)?;

    // 2. Record-prefix byte: ^ * + = ~ @ &
    let Some(prefix) = cursor.peek() else {
        return Err(CodecError::new(CodecErrorKind::UnexpectedEnd, cursor.pos()));
    };

    let record = match prefix {
        b'^' => {
            cursor.advance();
            Record::Result(parse_result_record(&mut cursor, token)?)
        }
        b'*' => {
            cursor.advance();
            Record::Exec(parse_async_record(&mut cursor, token)?)
        }
        b'+' => {
            cursor.advance();
            Record::Status(parse_async_record(&mut cursor, token)?)
        }
        b'=' => {
            cursor.advance();
            Record::Notify(parse_async_record(&mut cursor, token)?)
        }
        b'~' | b'@' | b'&' => {
            if token.is_some() {
                return Err(CodecError::new(CodecErrorKind::StreamRecordHasToken, 0));
            }
            cursor.advance();
            let sr = parse_stream_record(&mut cursor)?;
            match prefix {
                b'~' => Record::Console(sr),
                b'@' => Record::Target(sr),
                _ => Record::Log(sr),
            }
        }
        other => {
            return Err(CodecError::new(
                CodecErrorKind::InvalidRecordPrefix { found: other },
                cursor.pos(),
            ));
        }
    };

    if !cursor.at_end() {
        return Err(CodecError::new(
            CodecErrorKind::TrailingGarbage,
            cursor.pos(),
        ));
    }

    Ok(record)
}

/// Consume an optional `token` — a leading run of ASCII decimal digits.
///
/// Returns `Ok(None)` if the cursor is not positioned at a digit. Returns
/// `Err(TokenOverflow)` if the decimal value would overflow `u64`.
fn parse_optional_token(cursor: &mut Cursor<'_>) -> Result<Option<Token>, CodecError> {
    let start = cursor.pos();
    let mut value: u64 = 0;
    let mut any = false;
    while let Some(b) = cursor.peek() {
        if b.is_ascii_digit() {
            let digit = u64::from(b - b'0');
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .ok_or_else(|| CodecError::new(CodecErrorKind::TokenOverflow, start))?;
            cursor.advance();
            any = true;
        } else {
            break;
        }
    }
    // Token must be followed by a record-prefix byte. If we saw digits but
    // the next byte is not a valid prefix, that's a malformed record and
    // `parse_record` will report a clean error on the next byte. We don't
    // need to enforce it here.
    Ok(if any { Some(Token::new(value)) } else { None })
}
