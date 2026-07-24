#[inception::error]
enum FromWithContext {
    #[code("from.with_context")]
    #[description("The operation failed.")]
    Failed {
        #[from]
        #[caused_by]
        error: std::io::Error,
        detail: String,
    },
}

fn main() {}
