#[inception::error]
#[derive(Clone, PartialEq, Eq)]
enum GenericError<T>
where
    T: std::fmt::Display + std::fmt::Debug + Clone + PartialEq + Eq + Send + Sync + 'static,
{
    #[code("generic.failed")]
    #[description("The generic operation failed.")]
    Failed {
        value: T,
    },
}

fn main() {
    let error = GenericError::failed(42_u32);
    assert_eq!(error.code(), "generic.failed");
    assert_eq!(error, error.clone());
}
