use core::fmt;

use crate::InceptionError;

/// Formatting wrapper for a semantic creation trace.
pub struct Trace<'a> {
    error: &'a (dyn InceptionError + 'static),
}

impl<'a> Trace<'a> {
    #[must_use]
    pub fn new(error: &'a (dyn InceptionError + 'static)) -> Self {
        Self { error }
    }
}

impl fmt::Display for Trace<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut error = Some(self.error);
        let mut depth = 0;
        while let Some(layer) = error {
            if depth > 0 {
                writeln!(formatter, "\nCaused by:")?;
            }
            write!(formatter, "{}: ", layer.code())?;
            formatter.write_str(&layer.trace_message())?;
            write!(formatter, "\n    at {}", layer.created_at())?;
            error = layer.nested_error();
            depth += 1;
        }
        Ok(())
    }
}
