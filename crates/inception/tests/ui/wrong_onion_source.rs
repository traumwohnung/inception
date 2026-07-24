#[inception::error]
enum WrongInceptionSource {
    #[code("wrong.inception_source")]
    #[description("The operation failed.")]
    Failed {
        #[caused_by(inception)]
        error: std::io::Error,
    },
}

fn main() {}
