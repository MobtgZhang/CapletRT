//! Parent link for capability derivation tree

use crate::handle::HandleIndex;

/// Parent link in the derivation tree
#[derive(Debug, Clone, Copy)]
pub struct ParentLink {
    /// Index of the parent slot
    parent_index: Option<HandleIndex>,
    /// Whether this is a direct link
    direct: bool,
}

impl ParentLink {
    /// Create a root capability (no parent)
    pub const fn root() -> Self {
        Self {
            parent_index: None,
            direct: true,
        }
    }

    /// Create a child link to a parent
    pub const fn child(parent_index: HandleIndex) -> Self {
        Self {
            parent_index: Some(parent_index),
            direct: true,
        }
    }

    /// Create an indirect link (through intermediate node)
    pub const fn indirect(parent_index: HandleIndex) -> Self {
        Self {
            parent_index: Some(parent_index),
            direct: false,
        }
    }

    /// Check if this is a root capability
    pub const fn is_root(&self) -> bool {
        self.parent_index.is_none()
    }

    /// Get the parent index if any
    pub const fn parent_index(&self) -> Option<HandleIndex> {
        self.parent_index
    }

    /// Check if this is a direct child
    pub const fn is_direct(&self) -> bool {
        self.direct
    }
}

impl Default for ParentLink {
    fn default() -> Self {
        Self::root()
    }
}

/// Child link for tracking descendants
#[derive(Debug, Clone, Copy)]
pub struct ChildLink {
    /// Index of the child slot
    pub child_index: HandleIndex,
    /// Whether this link is still valid
    pub valid: bool,
}

impl ChildLink {
    /// Create a new child link
    pub const fn new(child_index: HandleIndex) -> Self {
        Self {
            child_index,
            valid: true,
        }
    }

    /// Invalidate this link
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Check if this link is valid
    pub const fn is_valid(&self) -> bool {
        self.valid
    }

    /// Get the child index
    pub const fn child_index(&self) -> HandleIndex {
        self.child_index
    }
}

/// Derivation tree node
#[derive(Debug, Clone, Copy)]
pub struct DerivationNode {
    /// Parent link
    pub parent: ParentLink,
    /// Number of active children
    pub child_count: u32,
    /// Total descendants (including non-direct)
    pub descendant_count: u32,
}

impl DerivationNode {
    /// Create a new node for a root capability
    pub const fn root() -> Self {
        Self {
            parent: ParentLink::root(),
            child_count: 0,
            descendant_count: 0,
        }
    }

    /// Create a new node for a child capability
    pub const fn child(parent_index: HandleIndex) -> Self {
        Self {
            parent: ParentLink::child(parent_index),
            child_count: 0,
            descendant_count: 0,
        }
    }

    /// Get the parent link
    pub const fn parent(&self) -> ParentLink {
        self.parent
    }

    /// Increment child count
    pub fn add_child(&mut self) {
        self.child_count += 1;
        self.descendant_count += 1;
    }

    /// Decrement child count
    pub fn remove_child(&mut self) {
        if self.child_count > 0 {
            self.child_count -= 1;
        }
        if self.descendant_count > 0 {
            self.descendant_count -= 1;
        }
    }

    /// Get the child count
    pub const fn child_count(&self) -> u32 {
        self.child_count
    }

    /// Get the total descendant count
    pub const fn descendant_count(&self) -> u32 {
        self.descendant_count
    }
}

impl Default for DerivationNode {
    fn default() -> Self {
        Self::root()
    }
}