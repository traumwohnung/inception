#![cfg(feature = "serde")]

#[inception::error(serde)]
enum SerializableError {
    #[code("serializable.failed")]
    #[description("The serializable operation failed.")]
    Failed { resource: String },
}

#[test]
fn generated_errors_and_runtime_metadata_serialize() {
    let error = SerializableError::failed("mailbox-1".to_owned());
    let error_json = serde_json::to_value(&error).unwrap();
    assert_eq!(error_json["error"]["Failed"]["resource"], "mailbox-1");
    assert_eq!(error_json["location"]["file"], file!());

    let descriptor_json = serde_json::to_value(SerializableError::CATALOG[0]).unwrap();
    assert_eq!(descriptor_json["code"], "serializable.failed");
}
