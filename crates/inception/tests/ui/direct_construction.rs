#[inception::error]
enum DirectConstruction {
    #[code("direct.construction")]
    #[description("The operation failed.")]
    Failed,
}

fn main() {
    let _ = DirectConstruction::Failed;
}
