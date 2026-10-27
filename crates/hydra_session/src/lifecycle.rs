//! The session generation counter.
//!
//! Recovered from `lifecycle::{get, advance}` at `0x101a809dc` / `0x101a809e8`,
//! which are six and five instructions over one `Arc`:
//!
//! ```text
//! get:     ldr x8,[x0] ; ldr w0,[x8,#0x10]            // Arc<Cell<u32>>
//! advance: ldr x8,[x0] ; ldr w9,[x8,#0x10]
//!          add w0,w9,#1 ; str w0,[x8,#0x10]           // returns the NEW value
//! ```
//!
//! So the counter lives in the payload of an `Arc` at `+0x10` (the first word
//! of the `ArcInner` payload) and `advance` bumps it and returns the value
//! *after* the bump.  `command::SessionHandle::generation` reads the same word
//! through its own `Arc` at `SessionHandle+0x10`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// A shared, monotonically increasing counter.
#[derive(Debug, Clone, Default)]
pub struct GenerationCell(Arc<AtomicU32>);

impl GenerationCell {
    /// A counter starting at zero.
    pub fn new() -> Self {
        GenerationCell(Arc::new(AtomicU32::new(0)))
    }

    /// The current value.
    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    /// Bump the counter and return the new value — the recovered behaviour
    /// (`add` before `str`, and the `add` result is what is returned).
    pub fn advance(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Bump the counter and return the value it had *before* the bump.
    pub fn take(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_returns_the_new_value_and_clones_share_it() {
        let a = GenerationCell::new();
        assert_eq!(a.get(), 0);
        assert_eq!(a.advance(), 1);
        assert_eq!(a.advance(), 2);
        let b = a.clone();
        assert_eq!(b.get(), 2);
        assert_eq!(b.take(), 2);
        assert_eq!(a.get(), 3);
    }
}
