use upio::registry::{build_downloader, build_uploader, BuildError, UploaderId};
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
fn unknown_urls_have_no_downloader() {
    assert_eq!(UploaderId::from_url("https://example.test/file"), None);
    assert_eq!(UploaderId::from_url("not a url"), None);
}

#[cfg(feature = "download")]
#[test]
fn fileditch_share_urls_are_registered() {
    let id = UploaderId::from_url("https://fileditchvids.st/alice/abc123/file.mp4").unwrap();
    assert_eq!(id.name(), "fileditch");
    assert!(id.supports_download());

    let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
    assert_eq!(downloader.name(), "fileditch");
}

#[cfg(feature = "download")]
#[test]
fn bunkr_share_urls_are_registered() {
    let id = UploaderId::from_url("https://bunkr.example/f/abc123xy").unwrap();
    assert_eq!(id.name(), "bunkr");
    assert!(id.supports_download());

    let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
    assert_eq!(downloader.name(), "bunkr");

    let album = UploaderId::from_url("https://bunkr.example/a/album42").unwrap();
    assert_eq!(album.name(), "bunkr");
}

#[cfg(feature = "download")]
#[test]
fn gofile_share_urls_are_registered() {
    let id = UploaderId::from_url("https://gofile.io/d/abc123").unwrap();
    assert_eq!(id.name(), "gofile");
    assert!(id.supports_download());

    let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
    assert_eq!(downloader.name(), "gofile");
}

#[cfg(feature = "download")]
#[test]
fn pixeldrain_share_urls_are_registered() {
    let id = UploaderId::from_url("https://pixeldrain.com/u/abc123").unwrap();
    assert_eq!(id.name(), "pixeldrain");
    assert!(id.supports_download());

    let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
    assert_eq!(downloader.name(), "pixeldrain");

    let list = UploaderId::from_url("https://pixeldra.in/l/abc123").unwrap();
    assert_eq!(list.name(), "pixeldrain");
}

#[cfg(feature = "download")]
#[test]
fn goonbox_share_urls_are_registered() {
    let id = UploaderId::from_url("https://goonbox.cr/img/abc123").unwrap();
    assert_eq!(id.name(), "goonbox");
    assert!(id.supports_download());

    let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
    assert_eq!(downloader.name(), "goonbox");

    let album = UploaderId::from_url("https://goonbox.cr/a/abc123").unwrap();
    assert_eq!(album.name(), "goonbox");

    let profile = UploaderId::from_url("https://goonbox.cr/u/alice").unwrap();
    assert_eq!(profile.name(), "goonbox");
}

#[cfg(feature = "download")]
#[test]
fn filester_share_urls_are_registered() {
    let id = UploaderId::from_url("https://filester.me/d/abc123").unwrap();
    assert_eq!(id.name(), "filester");
    assert!(id.supports_download());

    let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
    assert_eq!(downloader.name(), "filester");

    let folder = UploaderId::from_url("https://filester.me/f/folder1").unwrap();
    assert_eq!(folder.name(), "filester");

    let config = UploaderEndpointConfig {
        token: Some("key".to_string()),
        link_password: Some("password".to_string()),
        ..Default::default()
    };
    let downloader = build_downloader(folder, &config).unwrap();
    assert_eq!(downloader.name(), "filester");
}

#[test]
fn download_support_is_explicit_per_service() {
    for id in UploaderId::ALL {
        if id.supports_download() {
            continue;
        }
        let err = match build_downloader(*id, &UploaderEndpointConfig::default()) {
            Ok(_) => panic!("{} built a downloader without support", id.name()),
            Err(error) => error,
        };
        assert!(matches!(err, BuildError::Unsupported(_)));
    }
}

#[test]
fn every_id_has_distinct_info() {
    let mut crate_names = std::collections::HashSet::new();

    for id in UploaderId::ALL {
        let def = id.def();
        assert!(
            !def.display_name.is_empty(),
            "{} has no display name",
            id.name()
        );
        assert_eq!(def.name, id.name());
        assert!(
            crate_names.insert(def.crate_name),
            "{} reuses crate name '{}'",
            id.name(),
            def.crate_name
        );
    }
}

#[test]
fn build_bunkr_without_token_fails() {
    let id = UploaderId::from_name("bunkr").unwrap();
    let config = UploaderEndpointConfig::default();
    let err = match build_uploader(id, &config) {
        Ok(_) => panic!("bunkr built without a token"),
        Err(e) => e,
    };
    assert!(matches!(err, BuildError::MissingCredential(_)));
}

#[test]
fn build_goonbox_without_credentials_fails() {
    let id = UploaderId::from_name("goonbox").unwrap();
    let err = match build_uploader(id, &UploaderEndpointConfig::default()) {
        Ok(_) => panic!("goonbox built without a username"),
        Err(e) => e,
    };
    assert!(matches!(err, BuildError::MissingCredential(_)));
}

#[test]
fn build_goonbox_with_credentials_succeeds() {
    let id = UploaderId::from_name("goonbox").unwrap();
    let config = UploaderEndpointConfig {
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        ..Default::default()
    };
    let uploader = match build_uploader(id, &config) {
        Ok(uploader) => uploader,
        Err(e) => panic!("build failed: {}", e),
    };
    assert_eq!(uploader.name(), "goonbox");
}

#[test]
fn build_pixeldrain_without_token_fails() {
    let id = UploaderId::from_name("pixeldrain").unwrap();
    let config = UploaderEndpointConfig::default();
    let err = match build_uploader(id, &config) {
        Ok(_) => panic!("pixeldrain built without a token"),
        Err(e) => e,
    };
    assert!(matches!(err, BuildError::MissingCredential(_)));
}

#[test]
fn build_pixeldrain_with_token_succeeds() {
    let id = UploaderId::from_name("pixeldrain").unwrap();
    let config = UploaderEndpointConfig {
        token: Some("key".to_string()),
        ..Default::default()
    };
    let uploader = match build_uploader(id, &config) {
        Ok(uploader) => uploader,
        Err(e) => panic!("build failed: {}", e),
    };
    assert_eq!(uploader.name(), "pixeldrain");
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

#[test]
fn build_fileditch_with_token_succeeds() {
    let id = UploaderId::from_name("fileditch").unwrap();
    let config = UploaderEndpointConfig {
        token: Some("login-token".to_string()),
        ..Default::default()
    };
    let uploader = match build_uploader(id, &config) {
        Ok(uploader) => uploader,
        Err(e) => panic!("build failed: {}", e),
    };
    assert_eq!(uploader.name(), "fileditch");
    assert_eq!(
        id.def().credential,
        upio::registry::Credential::OptionalToken
    );
}
