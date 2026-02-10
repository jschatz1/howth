//! Arena allocator for AST nodes.
//!
//! Using arena allocation gives us:
//! - ~2-3x faster parsing (fewer individual allocations)
//! - Better cache locality (nodes are contiguous in memory)
//! - Faster cleanup (drop the arena, everything is freed)

use bumpalo::Bump;

/// Arena allocator for AST nodes.
pub struct Arena {
    bump: Bump,
}

impl Arena {
    /// Create a new arena with default capacity.
    pub fn new() -> Self {
        Self { bump: Bump::new() }
    }

    /// Create a new arena with the specified capacity in bytes.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bump: Bump::with_capacity(capacity),
        }
    }

    /// Allocate a value in the arena.
    #[inline]
    pub fn alloc<T>(&self, val: T) -> &T {
        self.bump.alloc(val)
    }

    /// Allocate a slice in the arena.
    #[inline]
    pub fn alloc_slice<T: Copy>(&self, slice: &[T]) -> &[T] {
        self.bump.alloc_slice_copy(slice)
    }

    /// Allocate a string in the arena.
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &str {
        self.bump.alloc_str(s)
    }

    /// Create a Vec that allocates in this arena.
    #[inline]
    pub fn vec<T>(&self) -> bumpalo::collections::Vec<'_, T> {
        bumpalo::collections::Vec::new_in(&self.bump)
    }

    /// Create a Vec with capacity that allocates in this arena.
    #[inline]
    pub fn vec_with_capacity<T>(&self, capacity: usize) -> bumpalo::collections::Vec<'_, T> {
        bumpalo::collections::Vec::with_capacity_in(capacity, &self.bump)
    }

    /// Get the underlying bump allocator.
    #[inline]
    pub fn bump(&self) -> &Bump {
        &self.bump
    }

    /// Reset the arena, deallocating all memory.
    pub fn reset(&mut self) {
        self.bump.reset();
    }

    /// Get the total bytes allocated.
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

/// A boxed value allocated in an arena.
/// This is just a reference, but named for clarity.
pub type Box<'a, T> = &'a T;

/// A vec allocated in an arena.
pub type Vec<'a, T> = bumpalo::collections::Vec<'a, T>;

/// A string slice (always borrowed from source or arena).
pub type Str<'a> = &'a str;
