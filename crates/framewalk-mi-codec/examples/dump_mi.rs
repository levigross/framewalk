//! `dump_mi` — read GDB/MI output bytes from stdin and print each parsed
//! record to stdout, one per line.
//!
//! Intended as a debugging aid for framewalk development: capture a real
//! `gdb -i=mi3` session to a file, then `cargo run --example dump_mi <
//! capture.mi` to see how the parser interprets each record. Errors are
//! printed to stderr with their byte offset but do not stop the tool; the
//! framer advances past the offending line and continues.
//!
//! This example uses `std` (it's compiled as a binary against the `std`
//! crate because cargo examples are standalone binaries), even though the
//! library itself is `#![no_std]`. The point of the library being `no_std`
//! is that *library consumers* pay nothing for std; examples and tests are
//! free to use it.

use std::io::{self, Read, Write};

use framewalk_mi_codec::parse_record;
use framewalk_mi_wire::{Frame, Framer};

fn main() -> io::Result<()> {
    let mut framer = Framer::new();
    let mut buf = [0u8; 4096];
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stderr = io::stderr();
    let mut stderr = stderr.lock();

    loop {
        let n = stdin.read(&mut buf)?;
        if n == 0 {
            break;
        }
        framer.push(&buf[..n]);
        while let Some(frame) = framer.pop() {
            match frame {
                Frame::GroupTerminator => {
                    writeln!(stdout, "(gdb)")?;
                }
                Frame::Line(bytes) => match parse_record(bytes) {
                    Ok(record) => writeln!(stdout, "{record:?}")?,
                    Err(e) => {
                        // Show the raw line for diagnostics. UTF-8 is not
                        // guaranteed, so we fall back to lossy for display.
                        writeln!(
                            stderr,
                            "parse error: {e}\n  raw: {:?}",
                            String::from_utf8_lossy(bytes)
                        )?;
                    }
                },
            }
        }
    }

    // Anything left in the framer without a closing newline is a truncated
    // line; report it but don't fail.
    if framer.pending_bytes() > 0 {
        writeln!(
            stderr,
            "warning: {} byte(s) of unterminated input at EOF",
            framer.pending_bytes()
        )?;
    }
    Ok(())
}
