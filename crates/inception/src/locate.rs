use core::fmt;

/// The source coordinate at which one semantic error layer was created.
///
/// Paths and coordinates are internal diagnostics. API projections must not
/// serialize them unless a separate, explicit policy chooses to do so.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Location {
    file: &'static str,
    line: u32,
    column: u32,
}

impl Location {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(file: &'static str, line: u32, column: u32) -> Self {
        Self { file, line, column }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn from_location(location: &'static core::panic::Location<'static>) -> Self {
        Self::new(location.file(), location.line(), location.column())
    }

    #[must_use]
    pub const fn file(self) -> &'static str {
        self.file
    }

    #[must_use]
    pub const fn line(self) -> u32 {
        self.line
    }

    #[must_use]
    pub const fn column(self) -> u32 {
        self.column
    }
}

impl fmt::Display for Location {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.file, self.line, self.column)
    }
}
