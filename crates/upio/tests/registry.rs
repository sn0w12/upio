use upio::registry::{build_uploader, BuildError, UploaderId};
use upio::UploaderEndpointConfig;

#[test]
fn all_ids_have_names_and_capabilities() {
    assert!(!UploaderId::ALL.is_empty());
    for id in UploaderId::ALL {
        assert_eq!(UploaderId::from_name(id.name()), Some(*id));
        let caps = id.capabilities();
        assert!(
            !caps.names().is_empty(),
            "{} has no capability names",
            id.name()
        );
    }
}

#[test]
fn unknown_name_resolves_to_none() {
    assert_eq!(UploaderId::from_name("nonexistent"), None);
}

#[test]
fn build_bunkr_without_token_fails() {
    let id = UploaderId::from_name("bunkr").unwrap();
    let config = UploaderEndpointConfig::default();
    let err = match build_uploader(id, &config) {
        Ok(_) => panic!("bunkr built without a token"),
        Err(e) => e,
    };
    assert!(matches!(err, BuildError::MissingToken(_)));
}

#[test]
fn build_fileditch_requires_no_token() {
    let id = UploaderId::from_name("fileditch").unwrap();
    let config = UploaderEndpointConfig::default();
    let uploader = match build_uploader(id, &config) {
        Ok(uploader) => uploader,
        Err(e) => panic!("build failed: {}", e),
    };
    assert_eq!(uploader.name(), "fileditch");
}
