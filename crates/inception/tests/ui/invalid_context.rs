#[inception::error]
enum InvalidContext {
    #[code("invalid.context")]
    #[description("The operation failed.")]
    Failed {
        #[hide]
        #[hide]
        value: String,
    },
}

fn main() {}
