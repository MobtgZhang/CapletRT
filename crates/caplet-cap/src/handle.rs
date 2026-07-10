//! Handle types for capability references

/// Handle index type - index into CSpace slot table
pub type HandleIndex = u32;

/// Generation counter for handle validity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(pub u32);

impl Generation {
    /// Create a new generation counter
    pub const fn new() -> Self {
        Self(0)
    }

    /// Increment the generation counter
    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

impl Default for Generation {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle is a reference to a capability in a CSpace
///
/// Handles are opaque references that can be safely passed around.
/// They consist of an index into the slot table and a generation
/// counter to detect stale references.
#[derive(Debug, PartialEq, Eq)]
pub struct Handle<T> {
    index: HandleIndex,
    generation: Generation,
    _phantom: core::marker::PhantomData<T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            generation: self.generation,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<T> Copy for Handle<T> {}

impl<T> Handle<T> {
    /// Create a new handle from index and generation
    pub const fn new(index: HandleIndex, generation: Generation) -> Self {
        Self {
            index,
            generation,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Get the handle index
    pub const fn index(&self) -> HandleIndex {
        self.index
    }

    /// Get the generation counter
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Validate that this handle matches the given index and generation
    pub fn validate(&self, expected_index: HandleIndex, expected_generation: &Generation) -> bool {
        self.index == expected_index && self.generation == *expected_generation
    }
}