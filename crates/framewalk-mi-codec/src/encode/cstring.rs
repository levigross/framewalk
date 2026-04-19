//! C-string encoder: writes a UTF-8 string into a byte buffer as a
//! `"..."` literal with the minimum necessary escaping.
//!
//! The encoder is the inverse of [`parse_cstring`](crate::parse::cstring::parse_cstring)
//! but it does not aim to round-trip byte-for-byte: the parser accepts a
//! liberal set of escape forms, while the encoder emits a single canonical
//! form for each byte. What is guaranteed is that
//! `parse(encode(s)) == Ok(s.to_owned())` for every valid UTF-8 `s`.

use alloc::vec::Vec;

/// Append a c-string literal encoding `s` to `out`.
///
/// The output begins with `"` and ends with `"`. Escapes follow the ISO C
/// rules the parser implements, favouring named escapes (`\n`, `\t`, etc)
/// over hex escapes for ASCII control characters to keep captures
/// human-readable.
pub fn encode_cstring(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for &b in s.as_bytes() {
        match b {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x07 => out.extend_from_slice(b"\\a"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0b => out.extend_from_slice(b"\\v"),
            0x0c => out.extend_from_slice(b"\\f"),
            // Other C0 control bytes: hex-escape for unambiguous round-trip.
            0x00..=0x1f | 0x7f => {
                out.push(b'\\');
                out.push(b'x');
                out.push(hex_nibble(b >> 4));
                out.push(hex_nibble(b & 0x0f));
            }
            // Printable ASCII and UTF-8 continuation/lead bytes: pass through.
            _ => out.push(b),
        }
    }
    out.push(b'"');
}

const fn hex_nibble(n: u8) -> u8 {
    match n {
        0..=9 => b'0' + n,
        10..=15 => b'a' + (n - 10),
        _ => b'?',
    }
}
