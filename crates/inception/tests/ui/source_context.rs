#[inception::error]
enum SourceContext {
    #[code("source.context")]
    #[description("The operation failed.")]
    Failed {
        #[hide]
        #[caused_by]
        error: std::io::Error,
    },
}

fn main() {}
