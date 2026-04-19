//! Serialisation of outbound [`MiCommand`](crate::command::MiCommand)s into
//! the byte form that GDB expects on stdin.
//!
//! Entry point: [`encode_command`]. The encoder never fails; its output is a
//! sequence of bytes ready to write to a transport, terminated by a single
//! `\n`. Token assignment is the protocol layer's responsibility and is
//! passed in here as an argument.

pub mod cstring;

use alloc::vec::Vec;

use crate::ast::Token;
use crate::command::{CommandOption, MiCommand};
use crate::encode::cstring::encode_cstring;

/// Serialise a command with an optional token prefix into `out`.
///
/// The produced bytes form a complete MI command line **including** the
/// trailing newline, so callers can write the whole buffer to the transport
/// in one shot.
///
/// Quoting rules (mirroring the GDB input grammar):
/// - Option names are emitted as `-name` (bare; GDB rejects quoting here).
/// - Option values and positional parameters are emitted as bare
///   `non-blank-sequence`s when they contain none of: space, tab, newline,
///   double-quote, backslash, and are non-empty. Otherwise they are wrapped
///   in a c-string literal with C-style escapes applied.
/// - A `--` separator is emitted between options and parameters when the
///   first parameter starts with `-`, so GDB doesn't misinterpret it as
///   another option. This matches the spec's documented `" --"` separator.
pub fn encode_command(token: Option<Token>, cmd: &MiCommand, out: &mut Vec<u8>) {
    if let Some(t) = token {
        // u64 → decimal digits; manual loop keeps the zero-dep contract.
        write_u64(out, t.get());
    }

    out.push(b'-');
    out.extend_from_slice(cmd.operation.as_bytes());

    for opt in &cmd.options {
        out.push(b' ');
        encode_option(opt, out);
    }

    if should_emit_double_dash(cmd) {
        out.extend_from_slice(b" --");
    }

    for param in &cmd.parameters {
        out.push(b' ');
        encode_parameter(param, out);
    }

    out.push(b'\n');
}

fn encode_option(opt: &CommandOption, out: &mut Vec<u8>) {
    out.push(b'-');
    out.extend_from_slice(opt.name.as_bytes());
    if let Some(value) = &opt.value {
        out.push(b' ');
        encode_parameter(value, out);
    }
}

fn encode_parameter(param: &str, out: &mut Vec<u8>) {
    if is_non_blank_sequence(param) {
        out.extend_from_slice(param.as_bytes());
    } else {
        encode_cstring(param, out);
    }
}

/// A `non-blank-sequence` is a non-empty run of bytes with no space, tab,
/// newline, double-quote, or backslash. Anything else must be c-string-quoted.
fn is_non_blank_sequence(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    !s.bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\\'))
}

/// Whether to emit the `--` options/parameters separator. The GDB input
/// grammar uses `--` to disambiguate a parameter that would otherwise look
/// like an option because it starts with `-`.
fn should_emit_double_dash(cmd: &MiCommand) -> bool {
    cmd.parameters.first().is_some_and(|p| p.starts_with('-'))
}

fn write_u64(out: &mut Vec<u8>, mut n: u64) {
    if n == 0 {
        out.push(b'0');
        return;
    }
    // u64::MAX has 20 decimal digits.
    let mut buf = [0u8; 20];
    let mut len = 0;
    while n != 0 {
        buf[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    // Reverse.
    for i in (0..len).rev() {
        out.push(buf[i]);
    }
}
