#[inception::error]
enum FromWithoutSource {
    #[code("from.without_source")]
    #[description("The operation failed.")]
    Failed {
        #[from]
        value: String,
    },
}

fn main() {}
