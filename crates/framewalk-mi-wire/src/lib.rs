//! `framewalk-mi-wire` — byte-level framer for the GDB/MI v3 protocol.
//!
//! This crate is `#![no_std]` + `alloc` and has zero external dependencies.
//! It consumes a raw byte stream from a GDB subprocess (chunked however the
//! OS chooses to deliver it) and emits [`Frame`] events for each complete MI
//! output line plus the `(gdb)` response-group terminator. It does **not**
//! understand MI grammar — that's the codec layer's job.
//!
//! The public surface is intentionally tiny: [`Framer`] and [`Frame`]. See
//! the [`Framer`] docs for the usage pattern and borrow semantics.

#![no_std]

extern crate alloc;

// `proptest` macros expand to code that references `std::panic` / `std::format`.
// Bringing `std` into scope during test builds only keeps the non-test crate
// graph std-free while still letting the test harness use proptest freely.
#[cfg(test)]
extern crate std;

mod frame;
mod framer;

pub use frame::Frame;
pub use framer::Framer;
