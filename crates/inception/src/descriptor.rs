use core::{error::Error as StdError, fmt};

/// A stable entry in the catalog generated for an error type.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorDescriptor {
    code: &'static str,
    description: &'static str,
}

impl ErrorDescriptor {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(code: &'static str, description: &'static str) -> Self {
        Self { code, description }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        self.description
    }
}

/// A generated builder was used without providing one of its fields.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildError {
    field: &'static str,
}

impl BuildError {
    #[doc(hidden)]
    #[must_use]
    pub const fn missing(field: &'static str) -> Self {
        Self { field }
    }

    #[must_use]
    pub const fn field(self) -> &'static str {
        self.field
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "missing cause field `{}`", self.field)
    }
}

impl StdError for BuildError {}
