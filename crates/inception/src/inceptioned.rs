use alloc::{borrow::Cow, format};
use core::{error::Error as StdError, fmt};

use crate::{InceptionError, Location};

/// An ordinary Rust error captured as one Inception error.
///
/// Its error value is preserved as the standard source; the wrapper supplies
/// the semantic creation location needed for a complete Inception trace.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Inceptioned<E> {
    error: E,
    location: Location,
}

impl<E> Inceptioned<E> {
    /// Wrap an error at the caller's source location.
    #[track_caller]
    #[must_use]
    pub fn new(error: E) -> Self {
        Self::new_at(
            error,
            Location::from_location(core::panic::Location::caller()),
        )
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn new_at(error: E, location: Location) -> Self {
        Self { error, location }
    }

    #[must_use]
    pub const fn error(&self) -> &E {
        &self.error
    }

    #[must_use]
    pub const fn location(&self) -> Location {
        self.location
    }

    #[must_use]
    pub fn into_error(self) -> E {
        self.error
    }
}

impl<E: StdError> fmt::Display for Inceptioned<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("An external error occurred.")
    }
}

impl<E: StdError + Send + Sync + 'static> fmt::Debug for Inceptioned<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Inceptioned")
            .field("code", &self.code())
            .field("description", &InceptionError::description(self))
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}

impl<E: StdError + Send + Sync + 'static> StdError for Inceptioned<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.error)
    }
}

impl<E: StdError + Send + Sync + 'static> InceptionError for Inceptioned<E> {
    fn code(&self) -> &'static str {
        "inception.external"
    }

    fn description(&self) -> &'static str {
        "An external error occurred."
    }

    fn trace_message(&self) -> Cow<'static, str> {
        Cow::Owned(format!("External error ({})", core::any::type_name::<E>()))
    }

    fn created_at(&self) -> Location {
        self.location
    }

    fn nested_error(&self) -> Option<&(dyn InceptionError + 'static)> {
        None
    }
}

#[doc(hidden)]
pub fn external_source<E: StdError + 'static>(error: &E) -> &(dyn StdError + 'static) {
    error
}
