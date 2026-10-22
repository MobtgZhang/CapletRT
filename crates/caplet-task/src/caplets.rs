//! Caplets - capability and service container for tasks.

use alloc::boxed::Box;
use alloc::sync::Arc;
use caplet_cap::CapSet;
use core::any::Any;
use hashbrown::HashMap;

/// A type-erased service object.
///
/// Services are stored behind `Arc` so a registry lookup does not
/// clone or move the underlying object. The `Send + Sync` bound is
/// required because a service may be looked up from any task or CPU.
type ServiceObject = Arc<dyn Any + Send + Sync>;

/// Caplets - container for task capabilities
pub struct Caplets {
    /// The capability set
    caps: Arc<CapSet>,
    /// Named, type-erased service registry.
    services: HashMap<&'static str, ServiceObject>,
}

impl Caplets {
    /// Create a new empty caplets container
    pub fn new() -> Self {
        Self {
            caps: Arc::new(CapSet::new()),
            services: HashMap::new(),
        }
    }

    /// Look up a service by name and type.
    ///
    /// Returns `None` when the name is not registered or when it is
    /// registered with a different concrete type. The returned `Arc`
    /// keeps the service alive even if the registry is later mutated.
    pub fn service<T>(&self, name: &str) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        let service = self.services.get(name)?;
        Arc::clone(service).downcast::<T>().ok()
    }

    /// Look up a service by name and return a borrowed reference.
    ///
    /// This avoids an atomic reference-count operation on hot paths.
    /// The borrow is tied to `self`, so the caller cannot retain it
    /// after the registry is dropped.
    pub fn service_ref<T>(&self, name: &str) -> Option<&T>
    where
        T: Any + Send + Sync,
    {
        self.services.get(name)?.downcast_ref::<T>()
    }

    /// Register a service under `name`.
    ///
    /// If the name was already registered, the old object is replaced
    /// and returned when it has the same concrete type `T`. Replacing
    /// a service with a different type behaves as an eviction: the old
    /// object is dropped and `None` is returned. Keeping one type per
    /// name makes lookup failures deterministic and prevents a stale
    /// registration from silently winning.
    pub fn register_service<T>(&mut self, name: &'static str, service: T) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        let previous = self.services.insert(name, Arc::new(service));
        previous.and_then(|old| old.downcast::<T>().ok())
    }

    /// Remove a service, returning the type-erased object if present.
    pub fn remove_service(&mut self, name: &str) -> Option<Box<dyn Any + Send + Sync>> {
        self.services
            .remove(name)
            .map(|service| Box::new(service) as Box<dyn Any + Send + Sync>)
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    #[test]
    fn service_lookup_is_name_and_type_checked() {
        let mut caps = Caplets::new();
        assert!(caps.register_service("uart:1", 0x4001_1000u32).is_none());

        let uart = caps.service::<u32>("uart:1").expect("registered service");
        assert_eq!(*uart, 0x4001_1000);
        assert_eq!(caps.service_ref::<u32>("uart:1"), Some(&0x4001_1000));

        assert!(caps.service::<u64>("uart:1").is_none());
        assert!(caps.service::<u32>("uart:2").is_none());
        assert!(caps.has_service("uart:1"));
        assert_eq!(caps.service_count(), 1);
    }

    #[test]
    fn duplicate_registration_reports_previous_value() {
        let mut caps = Caplets::new();
        assert!(caps.register_service("sensor", 1u8).is_none());
        let old = caps.register_service("sensor", 2u8);
        assert_eq!(old.as_deref(), Some(&1));
        assert_eq!(caps.service::<u8>("sensor").as_deref(), Some(&2));
    }

    #[test]
    fn replacing_with_another_type_evicts_the_old_entry() {
        let mut caps = Caplets::new();
        caps.register_service("slot", 7u32);
        assert!(caps.register_service("slot", String::from("new")).is_none());
        assert!(caps.service::<u32>("slot").is_none());
        assert_eq!(
            caps.service::<String>("slot").map(|s| s.to_string()),
            Some(String::from("new"))
        );
    }

    #[test]
    fn remove_service_releases_the_slot() {
        let mut caps = Caplets::new();
        caps.register_service("fs", 3usize);
        assert!(caps.remove_service("fs").is_some());
        assert!(!caps.has_service("fs"));
        assert!(caps.service::<usize>("fs").is_none());
    }
}
