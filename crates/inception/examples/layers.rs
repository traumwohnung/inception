//! A small end-to-end example of typed, nested errors.

#[inception::error]
enum NetworkError {
    #[code("network.timeout")]
    #[description("The inventory service timed out.")]
    Timeout,
}

#[inception::error]
enum InventoryError {
    #[code("inventory.request")]
    #[description("The inventory request failed.")]
    Request {
        #[caused_by(inception)]
        error: NetworkError,
    },
}

#[inception::error]
enum CheckoutError {
    #[code("checkout.reservation")]
    #[description("The item could not be reserved.")]
    Reservation {
        #[caused_by(inception)]
        error: InventoryError,
        item: String,
        attempt: u32,
        #[hide]
        service_token: String,
    },
}

#[allow(clippy::result_large_err)]
fn reserve(item: &str) -> Result<(), CheckoutError> {
    let network = NetworkError::timeout();
    let inventory = inception::locate!(InventoryError::Request { error: network });

    Err(CheckoutError::reservation(
        inventory,
        item.to_owned(),
        2,
        "never printed".to_owned(),
    ))
}

fn main() {
    let error = reserve("book-42").expect_err("the example deliberately fails");

    println!("code: {}", error.code());
    println!("description: {}", error.description());
    println!("\ntrace:\n{}", error.trace());

    println!("\nclassified context:");
    for field in error.entries() {
        println!("  context {:<12}: {}", field.key(), field.value());
    }

    assert_eq!(error.description(), "The item could not be reserved.");
    assert!(!format!("{error:?}").contains("never printed"));
}
