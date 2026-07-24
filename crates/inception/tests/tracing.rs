#![cfg(feature = "tracing")]

#[inception::error]
enum TracedError {
    #[code("traced.failed")]
    Failed,
}

#[test]
fn tracing_macros_accept_static_and_dynamic_levels() {
    let error = TracedError::failed();
    inception::tracing::error!(error);
    inception::tracing::warn!(error);
    inception::tracing::info!(error);
    inception::tracing::log!(inception::tracing::Level::DEBUG, error);
}
