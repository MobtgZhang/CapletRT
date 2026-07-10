//! Caplets - capability container for tasks

use caplet_cap::CapSet;
use alloc::sync::Arc;
use hashbrown::HashMap;

/// Caplets - container for task capabilities
pub struct Caplets {
    /// The capability set
    caps: Arc<CapSet>,
    /// Service registry
    services: HashMap<&'static str, usize>,
}

impl Caplets {
    /// Create a new empty caplets container
    pub fn new() -> Self {
        Self {
            caps: Arc::new(CapSet::new()),
            services: HashMap::new(),
        }
    }

    /// Lookup a service by name
    pub fn service<T>(&self, name: &str) -> Option<T>
    where
        T: 'static,
    {
        let _ = name;
        None
    }

    /// Register a service
    pub fn register_service(&mut self, name: &'static str) {
        let key = name.as_ptr() as usize;
        self.services.insert(name, key);
    }

    /// Check if a service exists
    pub fn has_service(&self, name: &str) -> bool {
        self.services.contains_key(name)
    }

    /// Get the number of registered services
    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    /// Get a reference to the capability set
    pub fn caps(&self) -> &CapSet {
        &self.caps
    }
}

impl Default for Caplets {
    fn default() -> Self {
        Self::new()
    }
}