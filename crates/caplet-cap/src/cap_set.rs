//! Capability Set - ordered collections of capabilities by name

use crate::cap::Cap;
use alloc::string::String;
use core::borrow::Borrow;
use hashbrown::HashMap;

/// Type-erased capability reference
pub type ErasedCap = Cap<()>;

/// Marker trait for capability types
pub trait AnyCap: 'static {
    /// Get the type name for debugging
    fn type_name(&self) -> &'static str;
}

impl<T: 'static> AnyCap for T {
    fn type_name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
}

/// A set of capabilities indexed by string keys
pub struct CapSet {
    caps: HashMap<CapKey, ErasedCap>,
    next_id: u32,
}

impl CapSet {
    /// Create a new empty capability set
    pub fn new() -> Self {
        Self {
            caps: HashMap::new(),
            next_id: 1,
        }
    }

    /// Insert a capability with the given key
    pub fn insert(&mut self, key: impl Into<CapKey>, cap: ErasedCap) -> Option<ErasedCap> {
        self.caps.insert(key.into(), cap)
    }

    /// Get a capability by key
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&ErasedCap>
    where
        CapKey: Borrow<Q>,
        Q: Eq + core::hash::Hash,
    {
        self.caps.get(key)
    }

    /// Remove a capability by key
    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<ErasedCap>
    where
        CapKey: Borrow<Q>,
        Q: Eq + core::hash::Hash,
    {
        self.caps.remove(key)
    }

    /// Check if the set contains a key
    pub fn contains<Q: ?Sized>(&self, key: &Q) -> bool
    where
        CapKey: Borrow<Q>,
        Q: Eq + core::hash::Hash,
    {
        self.caps.contains_key(key)
    }

    /// Get the number of capabilities in the set
    pub fn len(&self) -> usize {
        self.caps.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }

    /// Iterate over all capabilities
    pub fn iter(&self) -> impl Iterator<Item = (&CapKey, &ErasedCap)> {
        self.caps.iter()
    }

    /// Generate a new unique ID
    pub fn generate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Default for CapSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Key type for capability lookup
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapKey {
    /// The full key path
    path: String,
}

impl CapKey {
    /// Create a new capability key from a path string
    pub fn new(path: &str) -> Self {
        Self {
            path: String::from(path),
        }
    }

    /// Get the path string
    pub fn as_str(&self) -> &str {
        &self.path
    }

    /// Get the parent key
    pub fn parent(&self) -> Option<CapKey> {
        self.path.rsplit_once('/').map(|(p, _)| CapKey::new(p))
    }

    /// Get the final component
    pub fn name(&self) -> &str {
        self.path.rsplit_once('/').map(|(_, n)| n).unwrap_or(&self.path)
    }
}

impl<T: AsRef<str>> From<T> for CapKey {
    fn from(s: T) -> Self {
        Self::new(s.as_ref())
    }
}