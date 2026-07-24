#![no_std]

//! Typed, nested errors with semantic creation traces.
//!
//! `inception` defines error mechanics only: stable codes, safe descriptions,
//! typed sources, and the location at which every semantic layer was created.
//! Application policies stay outside the error value.
//!
//! ```
//! use inception::{InceptionError as _, locate};
//!
//! #[inception::error]
//! enum TransportError {
//!     #[code("transport.timeout")]
//!     #[description("The transport timed out.")]
//!     Timeout,
//! }
//!
//! #[inception::error]
//! enum OperationError {
//!     #[code("operation.dependency")]
//!     #[description("The operation's dependency failed.")]
//!     Dependency {
//!         #[caused_by(inception)]
//!         error: TransportError,
//!     },
//! }
//!
//! let nested = TransportError::timeout();
//! let error = locate!(OperationError::Dependency { error: nested });
//! assert!(error.trace().to_string().contains("operation.dependency"));
//! ```
//!
//! Direct variant construction is deliberately unavailable, so callers cannot
//! bypass creation-site capture:
//!
//! ```compile_fail
//! #[inception::error]
//! enum ExampleError {
//!     #[code("example.failed")]
//!     #[description("The example failed.")]
//!     Failed,
//! }
//!
//! let _ = ExampleError::Failed;
//! ```

mod descriptor;
mod entry;
mod inceptioned;
mod locate;
mod trace;

pub extern crate alloc;

#[doc(hidden)]
pub use alloc as __alloc;

#[cfg(feature = "tracing")]
pub mod tracing;

#[cfg(feature = "serde")]
#[doc(hidden)]
pub use serde;

pub use descriptor::{BuildError, ErrorDescriptor};
pub use entry::Entry;
pub use inception_derive::{bail, bail_err, error, locate, locate_err};
pub use inceptioned::Inceptioned;
#[doc(hidden)]
pub use inceptioned::external_source;
pub use locate::Location;
pub use trace::Trace;

use alloc::{borrow::Cow, vec::Vec};
use core::error::Error as StdError;

/// A typed error that can be rendered as an Inception trace.
///
/// Procedurally generated errors and Inceptioned implement this trait. Any
/// application-defined error shape can implement it directly.
pub trait InceptionError: StdError + Send + Sync + 'static {
    /// Stable class identity.
    fn code(&self) -> &'static str;

    /// Static, safe description suitable as an API fallback message.
    fn description(&self) -> &'static str;

    /// Safe message shown for this error in a semantic trace.
    #[must_use]
    fn trace_message(&self) -> Cow<'static, str> {
        Cow::Borrowed(InceptionError::description(self))
    }

    /// Where this error was created.
    fn created_at(&self) -> Location;

    /// The nested Inception error that caused this error, if any.
    fn nested_error(&self) -> Option<&(dyn InceptionError + 'static)>;

    /// Entries attached to this error.
    #[must_use]
    fn entries(&self) -> Vec<Entry> {
        Vec::new()
    }

    /// Format this error and its nested cause chain as a semantic trace.
    #[must_use]
    fn trace(&self) -> Trace<'_>
    where
        Self: Sized,
    {
        Trace::new(self)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn site_formats_as_source_coordinate() {
        assert_eq!(
            Location::new("src/example.rs", 12, 7).to_string(),
            "src/example.rs:12:7"
        );
    }
}
