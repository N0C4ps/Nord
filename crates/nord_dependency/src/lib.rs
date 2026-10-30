//! Typed dependency registration and resolution for Nord Tech applications.
//!
//! Register concrete service types during application composition, then pass the
//! resolved `Arc<T>` values into constructors. Keep the container at the
//! composition root instead of making it a global service locator.

use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

type ErasedService = Arc<dyn Any + Send + Sync>;

/// Errors raised while registering or resolving a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyError {
    /// A concrete service type was registered more than once.
    AlreadyRegistered { type_name: &'static str },
    /// No service of the requested concrete type was registered.
    Missing { type_name: &'static str },
}

impl Display for DependencyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered { type_name } => {
                write!(formatter, "dependency `{type_name}` is already registered")
            }
            Self::Missing { type_name } => {
                write!(formatter, "dependency `{type_name}` is not registered")
            }
        }
    }
}

impl Error for DependencyError {}

/// Mutable composition phase for registering shared services.
#[derive(Default)]
pub struct DependencyBuilder {
    services: HashMap<TypeId, ErasedService>,
}

impl DependencyBuilder {
    /// Create an empty dependency builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a shared service by its concrete type.
    ///
    /// Each concrete type may be registered once. Use a small local wrapper
    /// type when multiple values of the same external type or trait interface
    /// need distinct registrations.
    ///
    /// # Errors
    ///
    /// Returns [`DependencyError::AlreadyRegistered`] if this service type is
    /// already present in the builder.
    pub fn register<T>(&mut self, service: T) -> Result<(), DependencyError>
    where
        T: Any + Send + Sync,
    {
        self.register_shared(Arc::new(service))
    }

    /// Register an already shared service, preserving its `Arc` allocation.
    ///
    /// # Errors
    ///
    /// Returns [`DependencyError::AlreadyRegistered`] if this service type is
    /// already present in the builder.
    pub fn register_shared<T>(&mut self, service: Arc<T>) -> Result<(), DependencyError>
    where
        T: Any + Send + Sync,
    {
        let service_type = TypeId::of::<T>();
        if self.services.contains_key(&service_type) {
            return Err(DependencyError::AlreadyRegistered { type_name: type_name::<T>() });
        }

        self.services.insert(service_type, service);
        Ok(())
    }

    /// Freeze registrations into a thread-safe container for application use.
    #[must_use]
    pub fn build(self) -> DependencyContainer {
        DependencyContainer { services: self.services }
    }
}

/// Immutable, thread-safe collection of shared services.
#[derive(Default)]
pub struct DependencyContainer {
    services: HashMap<TypeId, ErasedService>,
}

impl DependencyContainer {
    /// Resolve a shared service by its concrete type.
    ///
    /// # Errors
    ///
    /// Returns [`DependencyError::Missing`] if this service type was not
    /// registered.
    pub fn get<T>(&self) -> Result<Arc<T>, DependencyError>
    where
        T: Any + Send + Sync,
    {
        let service = self
            .services
            .get(&TypeId::of::<T>())
            .cloned()
            .ok_or(DependencyError::Missing { type_name: type_name::<T>() })?;

        Arc::downcast::<T>(service)
            .map_err(|_| DependencyError::Missing { type_name: type_name::<T>() })
    }

    /// Return whether a concrete service type has been registered.
    #[must_use]
    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.services.contains_key(&TypeId::of::<T>())
    }

    /// Return the number of registered service types.
    #[must_use]
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// Return whether no services have been registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }
}
