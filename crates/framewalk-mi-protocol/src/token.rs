//! Monotonic token allocator for correlating commands with their responses.
//!
//! framewalk assigns tokens internally: callers never pick them. The
//! allocator starts at 1 (0 is reserved to mean "no token" in contexts
//! where that distinction matters) and increments on each [`allocate`]
//! call. At `u64::MAX` it wraps back to 1; hitting the wraparound is
//! theoretically impossible in practice (a session would need to issue
//! more than 18 quintillion commands) but is handled correctly for
//! provable correctness.

use framewalk_mi_codec::Token;

/// A monotonic source of unique [`Token`]s.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TokenAllocator {
    next: u64,
}

impl Default for TokenAllocator {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl TokenAllocator {
    /// Allocate a fresh token. After `u64::MAX - 1` tokens have been issued,
    /// the allocator wraps back to `1`.
    pub(crate) fn allocate(&mut self) -> Token {
        let t = self.next;
        self.next = self.next.checked_add(1).unwrap_or(1);
        Token::new(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_one() {
        let mut a = TokenAllocator::default();
        assert_eq!(a.allocate(), Token::new(1));
    }

    #[test]
    fn increments_monotonically() {
        let mut a = TokenAllocator::default();
        assert_eq!(a.allocate(), Token::new(1));
        assert_eq!(a.allocate(), Token::new(2));
        assert_eq!(a.allocate(), Token::new(3));
    }

    #[test]
    fn wraps_at_u64_max() {
        let mut a = TokenAllocator { next: u64::MAX };
        assert_eq!(a.allocate(), Token::new(u64::MAX));
        assert_eq!(a.allocate(), Token::new(1));
    }
}
