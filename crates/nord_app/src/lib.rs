//! Application lifecycle infrastructure for Nord Tech consumers.

use nord_dependency::{DependencyBuilder, DependencyContainer, DependencyError};
use std::any::{Any, type_name};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

/// Stable application-facing error for service registration and lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationServicesError {
    /// A concrete service type has already been registered.
    AlreadyRegistered { type_name: &'static str },
    /// No service of the requested concrete type is registered.
    Missing { type_name: &'static str },
}

impl Display for ApplicationServicesError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered { type_name } => {
                write!(formatter, "application service `{type_name}` is already registered")
            }
            Self::Missing { type_name } => {
                write!(formatter, "application service `{type_name}` is not registered")
            }
        }
    }
}

impl Error for ApplicationServicesError {}

fn map_dependency_error(error: &DependencyError) -> ApplicationServicesError {
    match error {
        DependencyError::AlreadyRegistered { type_name } => {
            ApplicationServicesError::AlreadyRegistered { type_name }
        }
        DependencyError::Missing { type_name } => ApplicationServicesError::Missing { type_name },
    }
}

/// Mutable service registration API owned by the application layer.
#[derive(Default)]
pub struct ApplicationServicesBuilder {
    inner: DependencyBuilder,
}

impl ApplicationServicesBuilder {
    /// Create an empty application service builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one shared service value by its concrete type.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationServicesError::AlreadyRegistered`] when this type
    /// has already been registered.
    pub fn register<T>(&mut self, service: T) -> Result<(), ApplicationServicesError>
    where
        T: Any + Send + Sync,
    {
        self.inner.register(service).map_err(|error| map_dependency_error(&error))
    }

    /// Register an existing shared service allocation.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationServicesError::AlreadyRegistered`] when this type
    /// has already been registered.
    pub fn register_shared<T>(&mut self, service: Arc<T>) -> Result<(), ApplicationServicesError>
    where
        T: Any + Send + Sync,
    {
        self.inner.register_shared(service).map_err(|error| map_dependency_error(&error))
    }

    /// Finish registration and create an immutable service collection.
    #[must_use]
    pub fn build(self) -> ApplicationServices {
        ApplicationServices { inner: self.inner.build() }
    }
}

/// Immutable, thread-safe application services.
pub struct ApplicationServices {
    inner: DependencyContainer,
}

impl ApplicationServices {
    /// Resolve a service by its concrete type.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationServicesError::Missing`] if the type was not
    /// registered.
    pub fn get<T>(&self) -> Result<Arc<T>, ApplicationServicesError>
    where
        T: Any + Send + Sync,
    {
        self.inner
            .get()
            .map_err(|_| ApplicationServicesError::Missing { type_name: type_name::<T>() })
    }

    /// Check whether a concrete service type has been registered.
    #[must_use]
    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.inner.contains::<T>()
    }
}
