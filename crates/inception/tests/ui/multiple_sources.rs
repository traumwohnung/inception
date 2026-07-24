#[inception::error]
enum Nested {
    #[code("nested.failed")]
    #[description("The nested operation failed.")]
    Failed,
}

#[inception::error]
enum MultipleSources {
    #[code("multiple.sources")]
    #[description("The operation failed.")]
    Failed {
        #[caused_by]
        error: std::io::Error,
        #[caused_by(inception)]
        nested: Nested,
    },
}

fn main() {}
