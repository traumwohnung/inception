use alloc::string::{String, ToString};
use core::fmt;

/// One entry attached to an Inception error.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    key: &'static str,
    value: String,
    hidden: bool,
}

impl Entry {
    #[doc(hidden)]
    #[must_use]
    pub fn display(key: &'static str, value: &dyn fmt::Display) -> Self {
        Self {
            key,
            value: value.to_string(),
            hidden: false,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn hide(key: &'static str) -> Self {
        Self {
            key,
            value: String::from("<hidden>"),
            hidden: true,
        }
    }

    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.hidden
    }
}
