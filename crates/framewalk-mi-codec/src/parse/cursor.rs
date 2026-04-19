//! A minimal byte cursor used throughout the parser.
//!
//! The cursor gives hand-written recursive-descent code the three primitives
//! it actually needs — peek, advance, and positional error reporting — and
//! nothing else. No backtracking, no slicing, no fancy combinators; the
//! grammar is LL(1) so every decision commits on the current byte.

use crate::error::{CodecError, CodecErrorKind, Expected};

pub(crate) struct Cursor<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    #[inline]
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    /// Current byte offset into the original input. Used when constructing
    /// [`CodecError`]s so error messages pinpoint the failure.
    #[inline]
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// `true` if the cursor has consumed every byte of the input.
    #[inline]
    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// The next byte without advancing.
    #[inline]
    pub(crate) fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    /// The byte `offset` positions ahead of the cursor without advancing.
    #[inline]
    pub(crate) fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    /// Advance the cursor by one byte. Returns the consumed byte, or `None`
    /// if the cursor was already at end-of-input.
    #[inline]
    pub(crate) fn advance(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    /// Consume the next byte if it equals `expected`; otherwise return an
    /// [`CodecErrorKind::UnexpectedByte`] error positioned at the cursor.
    pub(crate) fn expect_byte(&mut self, expected: u8) -> Result<(), CodecError> {
        match self.peek() {
            Some(b) if b == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(found) => Err(CodecError::new(
                CodecErrorKind::UnexpectedByte {
                    expected: Expected::Byte(expected),
                    found,
                },
                self.pos,
            )),
            None => Err(CodecError::new(CodecErrorKind::UnexpectedEnd, self.pos)),
        }
    }
}
