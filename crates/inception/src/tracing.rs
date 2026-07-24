//! Optional integration for emitting structured tracing events.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use crate::InceptionError;

pub use crate::{
    __inception_error as error, __inception_info as info, __inception_log as log,
    __inception_warn as warn,
};
#[doc(hidden)]
pub use ::tracing as __tracing;
pub use ::tracing::Level;

#[doc(hidden)]
pub struct EventFields {
    pub code: &'static str,
    pub description: &'static str,
    pub trace: String,
    pub context: String,
}

#[doc(hidden)]
#[must_use]
pub fn event_fields<E: InceptionError>(error: &E) -> EventFields {
    let mut context = Vec::new();
    let mut layer = Some(error as &(dyn InceptionError + 'static));
    while let Some(error) = layer {
        context.extend(
            error
                .entries()
                .into_iter()
                .map(|entry| format!("{}={}", entry.key(), entry.value())),
        );
        layer = error.nested_error();
    }
    EventFields {
        code: error.code(),
        description: InceptionError::description(error),
        trace: error.trace().to_string(),
        context: context.join(" "),
    }
}

/// Emit an Inception error at an explicit tracing level.
#[macro_export]
macro_rules! __inception_log {
    ($level:expr, $error:expr $(,)?) => {{
        let __inception_fields = $crate::tracing::event_fields(&$error);
        $crate::tracing::__tracing::event!(
            $level,
            error.code = __inception_fields.code,
            error.description = __inception_fields.description,
            error.trace = %__inception_fields.trace,
            error.context = %__inception_fields.context,
            "operation failed"
        );
    }};
}

/// Emit an Inception error at the ERROR level.
#[macro_export]
macro_rules! __inception_error {
    ($error:expr $(,)?) => {
        $crate::__inception_log!($crate::tracing::Level::ERROR, $error)
    };
}

/// Emit an Inception error at the WARN level.
#[macro_export]
macro_rules! __inception_warn {
    ($error:expr $(,)?) => {
        $crate::__inception_log!($crate::tracing::Level::WARN, $error)
    };
}

/// Emit an Inception error at the INFO level.
#[macro_export]
macro_rules! __inception_info {
    ($error:expr $(,)?) => {
        $crate::__inception_log!($crate::tracing::Level::INFO, $error)
    };
}
