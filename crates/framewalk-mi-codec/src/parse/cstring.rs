//! C-string parsing with escape-sequence decoding.
//!
//! This file is deliberately isolated from the rest of the parser because
//! escape decoding is where every hand-rolled MI parser quietly grows bugs.
//! Every edge case of the C string grammar has its own dedicated test.
//!
//! Supported escapes (per ISO C and the GDB manual's `c-string` production):
//!
//! | Escape     | Decodes to                                      |
//! |------------|-------------------------------------------------|
//! | `\"`       | `"` (ASCII 0x22)                                 |
//! | `\\`       | `\` (ASCII 0x5c)                                 |
//! | `\'`       | `'` (ASCII 0x27)                                 |
//! | `\?`       | `?` (ASCII 0x3f) — C trigraph escape             |
//! | `\a`       | bell (0x07)                                      |
//! | `\b`       | backspace (0x08)                                 |
//! | `\f`       | form feed (0x0c)                                 |
//! | `\n`       | newline (0x0a)                                   |
//! | `\r`       | carriage return (0x0d)                           |
//! | `\t`       | horizontal tab (0x09)                            |
//! | `\v`       | vertical tab (0x0b)                              |
//! | `\xHH`     | exactly two hex digits → one byte                |
//! | `\ooo`     | one to three octal digits → one byte             |
//!
//! Unknown escape letters (e.g. `\z`) are a hard parse error, matching the
//! ISO C rule. Embedded NUL bytes produced via `\0` or `\x00` are accepted;
//! Rust `String` permits them. Invalid UTF-8 in the decoded bytes is a
//! parser error, reported by the caller after this function returns.

use alloc::string::String;
use alloc::vec::Vec;

use crate::error::{CStringError, CodecError, CodecErrorKind};
use crate::parse::cursor::Cursor;

/// Parse a c-string at the cursor. The cursor must be positioned at the
/// opening `"`. On success the cursor ends one past the closing `"`.
///
/// The returned `String` contains the decoded contents with escape
/// sequences fully resolved (`\n` becomes a real newline, `\xff` becomes a
/// 0xff byte, etc). A non-UTF-8 decoding is reported as
/// [`CodecErrorKind::InvalidUtf8`] with the offset pointing at the opening
/// quote of the offending c-string.
pub(crate) fn parse_cstring(cursor: &mut Cursor<'_>) -> Result<String, CodecError> {
    let open_pos = cursor.pos();

    // Opening quote.
    match cursor.peek() {
        Some(b'"') => {
            cursor.advance();
        }
        _ => {
            return Err(CodecError::new(
                CodecErrorKind::InvalidCString(CStringError::MissingOpenQuote),
                open_pos,
            ));
        }
    }

    let mut out: Vec<u8> = Vec::new();

    loop {
        let Some(byte) = cursor.peek() else {
            return Err(CodecError::new(
                CodecErrorKind::InvalidCString(CStringError::MissingCloseQuote),
                open_pos,
            ));
        };

        match byte {
            b'"' => {
                // Closing quote.
                cursor.advance();
                break;
            }
            b'\\' => {
                decode_escape(cursor, &mut out)?;
            }
            other => {
                out.push(other);
                cursor.advance();
            }
        }
    }

    String::from_utf8(out).map_err(|_| CodecError::new(CodecErrorKind::InvalidUtf8, open_pos))
}

/// Decode one escape sequence starting at the current cursor position
/// (which must be pointing at a `\`). Advances the cursor past the entire
/// escape sequence and pushes the decoded bytes onto `out`.
fn decode_escape(cursor: &mut Cursor<'_>, out: &mut Vec<u8>) -> Result<(), CodecError> {
    let escape_pos = cursor.pos();
    debug_assert_eq!(cursor.peek(), Some(b'\\'));
    cursor.advance();

    let Some(next) = cursor.peek() else {
        return Err(CodecError::new(
            CodecErrorKind::InvalidCString(CStringError::TruncatedEscape),
            escape_pos,
        ));
    };

    match next {
        b'"' => {
            cursor.advance();
            out.push(b'"');
        }
        b'\\' => {
            cursor.advance();
            out.push(b'\\');
        }
        b'\'' => {
            cursor.advance();
            out.push(b'\'');
        }
        b'?' => {
            cursor.advance();
            out.push(b'?');
        }
        b'a' => {
            cursor.advance();
            out.push(0x07);
        }
        b'b' => {
            cursor.advance();
            out.push(0x08);
        }
        b'f' => {
            cursor.advance();
            out.push(0x0c);
        }
        b'n' => {
            cursor.advance();
            out.push(b'\n');
        }
        b'r' => {
            cursor.advance();
            out.push(b'\r');
        }
        b't' => {
            cursor.advance();
            out.push(b'\t');
        }
        b'v' => {
            cursor.advance();
            out.push(0x0b);
        }
        b'x' => {
            // Exactly two hex digits.
            cursor.advance();
            let hi = read_hex_digit(cursor, escape_pos)?;
            let lo = read_hex_digit(cursor, escape_pos)?;
            out.push((hi << 4) | lo);
        }
        b'0'..=b'7' => {
            // One to three octal digits, greedy.
            let mut value: u16 = 0;
            let mut digits = 0;
            while digits < 3 {
                match cursor.peek() {
                    Some(b @ b'0'..=b'7') => {
                        cursor.advance();
                        value = value * 8 + u16::from(b - b'0');
                        digits += 1;
                    }
                    _ => break,
                }
            }
            // `\ooo` can overflow one byte (e.g. `\777` = 511). In that
            // case we wrap to u8, matching GCC/Clang behaviour for
            // overlarge octal escapes in narrow char literals. Any
            // resulting lone continuation byte will fail the downstream
            // UTF-8 validation check and surface as InvalidUtf8, so
            // truncation here is intentional and not a data loss concern.
            #[allow(clippy::cast_possible_truncation)]
            out.push(value as u8);
        }
        other => {
            return Err(CodecError::new(
                CodecErrorKind::InvalidCString(CStringError::InvalidEscape { found: other }),
                escape_pos,
            ));
        }
    }
    Ok(())
}

/// Consume one ASCII hex digit from the cursor and return its nibble value.
fn read_hex_digit(cursor: &mut Cursor<'_>, escape_pos: usize) -> Result<u8, CodecError> {
    let Some(b) = cursor.peek() else {
        return Err(CodecError::new(
            CodecErrorKind::InvalidCString(CStringError::TruncatedEscape),
            escape_pos,
        ));
    };
    let nibble = match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => {
            return Err(CodecError::new(
                CodecErrorKind::InvalidCString(CStringError::InvalidHexDigit { found: b }),
                cursor.pos(),
            ));
        }
    };
    cursor.advance();
    Ok(nibble)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &[u8]) -> Result<String, CodecError> {
        let mut cursor = Cursor::new(input);
        parse_cstring(&mut cursor)
    }

    fn parse_ok(input: &[u8]) -> String {
        parse(input).unwrap_or_else(|e| panic!("unexpected error on {input:?}: {e}"))
    }

    // ---- Empty and simple content ----

    #[test]
    fn empty_string() {
        assert_eq!(parse_ok(b"\"\""), "");
    }

    #[test]
    fn plain_ascii() {
        assert_eq!(parse_ok(b"\"hello\""), "hello");
    }

    #[test]
    fn single_character() {
        assert_eq!(parse_ok(b"\"a\""), "a");
    }

    #[test]
    fn space_and_punct() {
        assert_eq!(parse_ok(b"\"hi, world!\""), "hi, world!");
    }

    // ---- Simple escapes ----

    #[test]
    fn escape_double_quote() {
        assert_eq!(parse_ok(b"\"a\\\"b\""), "a\"b");
    }

    #[test]
    fn escape_backslash() {
        assert_eq!(parse_ok(b"\"a\\\\b\""), "a\\b");
    }

    #[test]
    fn escape_single_quote() {
        assert_eq!(parse_ok(b"\"\\'\""), "'");
    }

    #[test]
    fn escape_question_mark() {
        assert_eq!(parse_ok(b"\"\\?\""), "?");
    }

    #[test]
    fn escape_newline() {
        assert_eq!(parse_ok(b"\"line1\\nline2\""), "line1\nline2");
    }

    #[test]
    fn escape_tab() {
        assert_eq!(parse_ok(b"\"a\\tb\""), "a\tb");
    }

    #[test]
    fn escape_carriage_return() {
        assert_eq!(parse_ok(b"\"a\\rb\""), "a\rb");
    }

    #[test]
    fn escape_bell_backspace_formfeed_vtab() {
        assert_eq!(parse_ok(b"\"\\a\""), "\u{07}");
        assert_eq!(parse_ok(b"\"\\b\""), "\u{08}");
        assert_eq!(parse_ok(b"\"\\f\""), "\u{0c}");
        assert_eq!(parse_ok(b"\"\\v\""), "\u{0b}");
    }

    // ---- Hex escapes ----

    #[test]
    fn hex_escape_lowercase() {
        assert_eq!(parse_ok(b"\"\\x41\""), "A");
    }

    #[test]
    fn hex_escape_uppercase() {
        assert_eq!(parse_ok(b"\"\\x4A\""), "J");
    }

    #[test]
    fn hex_escape_mixed_case() {
        assert_eq!(parse_ok(b"\"\\x4a\""), "J");
    }

    #[test]
    fn hex_escape_full_byte() {
        // 0xc3 0xa9 is UTF-8 for 'é'.
        assert_eq!(parse_ok(b"\"\\xc3\\xa9\""), "é");
    }

    #[test]
    fn hex_escape_null_byte() {
        let s = parse_ok(b"\"a\\x00b\"");
        assert_eq!(s.as_bytes(), b"a\x00b");
    }

    #[test]
    fn hex_escape_truncated_to_one_digit() {
        // `\x4"` — the closing quote is consumed as the second hex digit
        // attempt, which fails.
        let err = parse(b"\"\\x4\"").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::InvalidHexDigit { .. })
        ));
    }

    #[test]
    fn hex_escape_truncated_no_digits() {
        let err = parse(b"\"\\x\"").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::InvalidHexDigit { .. })
        ));
    }

    #[test]
    fn hex_escape_non_hex_digit() {
        let err = parse(b"\"\\xZZ\"").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::InvalidHexDigit { found: b'Z' })
        ));
    }

    // ---- Octal escapes ----

    #[test]
    fn octal_one_digit() {
        assert_eq!(parse_ok(b"\"\\0\"").as_bytes(), b"\x00");
    }

    #[test]
    fn octal_two_digits() {
        assert_eq!(parse_ok(b"\"\\07\"").as_bytes(), b"\x07");
    }

    #[test]
    fn octal_three_digits() {
        assert_eq!(parse_ok(b"\"\\101\""), "A"); // octal 101 = 65 = 'A'
    }

    #[test]
    fn octal_stops_at_non_octal() {
        // `\18` — the `8` is not octal, so only `\1` is consumed and `8`
        // follows as a literal.
        assert_eq!(parse_ok(b"\"\\18\"").as_bytes(), b"\x018");
    }

    #[test]
    fn octal_maxes_at_three_digits() {
        // `\1234` — three digits (123 octal = 83 = 'S') then literal '4'.
        assert_eq!(parse_ok(b"\"\\1234\""), "S4");
    }

    #[test]
    fn octal_overflow_wraps() {
        // `\400` is octal 256, which overflows one byte and wraps to 0x00.
        // GCC/Clang do the same in narrow char literals. We pick 400 rather
        // than the more obvious 777 because 0xff alone is not valid UTF-8
        // and would then trip the InvalidUtf8 check before the test could
        // observe the wrap.
        assert_eq!(parse_ok(b"\"\\400\"").as_bytes(), b"\x00");
    }

    #[test]
    fn octal_overflow_produces_invalid_utf8() {
        // `\777` wraps to 0xff which is a lone continuation byte: not
        // valid UTF-8. This documents that fail-closed UTF-8 validation
        // catches overflowed escapes rather than silently lossy-decoding.
        let err = parse(b"\"\\777\"").unwrap_err();
        assert_eq!(err.kind, CodecErrorKind::InvalidUtf8);
    }

    // ---- Mixed escape sequences ----

    #[test]
    fn many_escapes_in_one_string() {
        assert_eq!(
            parse_ok(b"\"\\\"quoted\\\" and \\\\backslash\\\\ and \\n newline\""),
            "\"quoted\" and \\backslash\\ and \n newline"
        );
    }

    #[test]
    fn escape_then_plain_then_escape() {
        assert_eq!(parse_ok(b"\"\\nfoo\\t\""), "\nfoo\t");
    }

    // ---- UTF-8 content ----

    #[test]
    fn raw_utf8_multibyte() {
        assert_eq!(parse_ok("\"café\"".as_bytes()), "café");
    }

    #[test]
    fn invalid_utf8_via_hex_escape() {
        // 0xc3 alone (without a continuation byte) is invalid UTF-8.
        let err = parse(b"\"\\xc3\"").unwrap_err();
        assert_eq!(err.kind, CodecErrorKind::InvalidUtf8);
    }

    // ---- Error modes ----

    #[test]
    fn missing_open_quote() {
        let err = parse(b"foo").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::MissingOpenQuote)
        ));
        assert_eq!(err.offset, 0);
    }

    #[test]
    fn missing_close_quote_empty() {
        let err = parse(b"\"").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::MissingCloseQuote)
        ));
    }

    #[test]
    fn missing_close_quote_with_content() {
        let err = parse(b"\"unterminated").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::MissingCloseQuote)
        ));
    }

    #[test]
    fn trailing_backslash_is_error() {
        // `"\\"` in source = `"\"` on the wire: opening quote, backslash,
        // closing quote. The backslash begins an escape sequence whose
        // next byte is the closing quote, which escapes as `"` — leaving
        // NO closing quote, so the c-string runs off the end of input.
        let err = parse(b"\"\\\"").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::MissingCloseQuote)
        ));
    }

    #[test]
    fn unknown_escape_letter() {
        let err = parse(b"\"\\z\"").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::InvalidEscape { found: b'z' })
        ));
    }

    #[test]
    fn truncated_escape_at_eof() {
        // `"\` — backslash with nothing after, not even a closing quote.
        let err = parse(b"\"\\").unwrap_err();
        assert!(matches!(
            err.kind,
            CodecErrorKind::InvalidCString(CStringError::TruncatedEscape)
        ));
    }

    // ---- Cursor positioning ----

    #[test]
    fn cursor_positioned_after_closing_quote() {
        // After parsing a c-string, the cursor should be one past the
        // closing `"` so the enclosing parser can read the next byte
        // (typically `,` or `}` or `]`).
        let input = b"\"hello\",x";
        let mut cursor = Cursor::new(input);
        let s = parse_cstring(&mut cursor).unwrap();
        assert_eq!(s, "hello");
        assert_eq!(cursor.peek(), Some(b','));
    }
}
