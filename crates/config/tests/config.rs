use std::io::Write;

use upio_config::{Config, ConfigKey, Settings};
use upio_core::preprocess::{PreprocessConfig, PreprocessRule, PreprocessStrategy};

#[test]
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    let mut config = Config::default();
    config.global.disabled_uploaders = vec!["filester".to_string()];
    config.bunkr = Some(Default::default());

    config.to_file(&path).unwrap();
    let loaded = Config::from_file(&path).unwrap();

    assert_eq!(
        loaded.global.disabled_uploaders,
        vec!["filester".to_string()]
    );
    assert!(loaded.bunkr.is_some());
}

#[test]
fn missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist.toml");

    assert!(Config::from_file(&path).is_err());
    assert!(Config::default().bunkr.is_none());
}

#[test]
fn preprocess_rules_parse_from_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let toml_str = r#"
        [bunkr]
        token = "abc"
        folder_id = "123"

        [bunkr.preprocess]
        enabled = true

        [[bunkr.preprocess.rules]]
        strategy = "split_video"

        [[bunkr.preprocess.rules]]
        strategy = "wrap_zip"
    "#;
    std::fs::File::create(&path)
        .unwrap()
        .write_all(toml_str.as_bytes())
        .unwrap();

    let config = Config::from_file(&path).unwrap();
    let bunkr = config.bunkr.expect("bunkr section parsed");

    assert_eq!(bunkr.token.as_deref(), Some("abc"));
    assert_eq!(bunkr.folder_id.as_deref(), Some("123"));
    let preprocess = bunkr.preprocess.expect("preprocess parsed");
    assert!(preprocess.enabled);
    assert_eq!(preprocess.rules.len(), 2);

    let video = &preprocess.rules[0];
    assert_eq!(video.strategy.def().id, "split_video");

    let zip = &preprocess.rules[1];
    assert_eq!(zip.strategy.def().id, "wrap_zip");
}

#[test]
fn get_and_set_values() {
    let mut config = Config::default();

    let token = ConfigKey::BunkrToken;
    assert_eq!(config.get_value(&token), "");
    config.set_value(&token, "sekret").unwrap();
    assert_eq!(config.get_value(&token), "sekret");

    config.set_value(&token, "none").unwrap();
    assert_eq!(config.get_value(&token), "");

    let disabled = ConfigKey::GlobalDisabledUploaders;
    config.set_value(&disabled, "bunkr, gofile").unwrap();
    assert_eq!(config.get_value(&disabled), "bunkr, gofile");

    assert!(config.set_value(&ConfigKey::FilesterFolderId, "7").is_ok());
    assert_eq!(
        config.filester.as_ref().unwrap().folder_id.as_deref(),
        Some("7")
    );
}

#[test]
fn download_credentials_are_typed_secrets() {
    let mut config = Config::default();

    config
        .set_value(&ConfigKey::GofileLinkPassword, "a b")
        .unwrap();
    assert_eq!(
        config.gofile.as_ref().unwrap().link_password.as_deref(),
        Some("a b")
    );
    assert!(ConfigKey::GofileLinkPassword.is_secret());

    config
        .set_value(&ConfigKey::FilesterLinkPassword, "folder password")
        .unwrap();
    assert_eq!(
        config.filester.as_ref().unwrap().link_password.as_deref(),
        Some("folder password")
    );
    assert!(ConfigKey::FilesterLinkPassword.is_secret());

    config.set_value(&ConfigKey::GoonboxToken, "key").unwrap();
    assert_eq!(
        config.goonbox.as_ref().unwrap().token.as_deref(),
        Some("key")
    );
    assert!(ConfigKey::GoonboxToken.is_secret());
}

#[test]
fn get_uploader_config_respects_preprocess() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let toml_str = r#"
        [gofile]
        token = "abc"

        [gofile.preprocess]
        enabled = true
    "#;
    std::fs::File::create(&path)
        .unwrap()
        .write_all(toml_str.as_bytes())
        .unwrap();

    let config = Config::from_file(&path).unwrap();
    let gofile = config.get_uploader_config("gofile");

    assert_eq!(gofile.token.as_deref(), Some("abc"));
    assert!(gofile.preprocess.expect("preprocess present").enabled);
}

#[test]
fn global_preprocess_applies_until_a_service_overrides_it() {
    let toml_str = r#"
        [preprocess]
        enabled = true

        [[preprocess.rules]]
        strategy = "normalize_name"

        [bunkr.preprocess]
        enabled = false
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();

    let gofile = config.preprocess_for("gofile");
    assert!(gofile.enabled);
    assert_eq!(gofile.rules.len(), 1);
    assert_eq!(gofile.rules[0].strategy.def().id, "normalize_name");

    let bunkr = config.preprocess_for("bunkr");
    assert!(!bunkr.enabled);
    assert!(bunkr.rules.is_empty());

    assert!(config.preprocess_override("bunkr").is_some());
    assert!(config.preprocess_override("gofile").is_none());
    assert!(config.endpoint_for("gofile").preprocess.unwrap().enabled);
}

#[test]
fn service_preprocess_replaces_the_global_rules() {
    let toml_str = r#"
        [preprocess]
        enabled = true

        [[preprocess.rules]]
        strategy = "normalize_name"

        [gofile.preprocess]
        enabled = true

        [[gofile.preprocess.rules]]
        strategy = "wrap_zip"
    "#;
    let config: Config = toml::from_str(toml_str).unwrap();

    let gofile = config.preprocess_for("gofile");
    assert_eq!(gofile.rules.len(), 1);
    assert_eq!(gofile.rules[0].strategy.def().id, "wrap_zip");
}

#[test]
fn serializing_omits_inherited_preprocess() {
    let mut config = Config::default();
    config.preprocess.enabled = true;
    config.preprocess.rules.push(PreprocessRule::new(
        PreprocessStrategy::by_id("wrap_zip").unwrap(),
    ));
    config.bunkr = Some(Default::default());

    let toml = config.to_toml().unwrap();
    assert!(toml.contains("[preprocess]"));
    assert!(!toml.contains("[bunkr.preprocess]"));
}

#[test]
fn preprocess_mut_seeds_the_override_from_global() {
    let mut config = Config::default();
    config.preprocess.enabled = true;
    config.preprocess.rules.push(PreprocessRule::new(
        PreprocessStrategy::by_id("wrap_zip").unwrap(),
    ));

    let preprocess = config.preprocess_mut("gofile").unwrap();
    assert!(preprocess.enabled);
    assert_eq!(preprocess.rules.len(), 1);
    preprocess.enabled = false;

    let gofile = config.preprocess_for("gofile");
    assert!(!gofile.enabled);
    assert_eq!(gofile.rules.len(), 1);
    assert!(config.preprocess_override("gofile").is_some());
}

#[test]
fn settings_extract_keeps_preprocess_layers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[preprocess]\nenabled = true\n\n[bunkr.preprocess]\nenabled = false\n",
    )
    .unwrap();

    let config = Settings::load_from(&path).unwrap().extract().unwrap();
    assert!(config.preprocess.enabled);
    assert!(!config.preprocess_for("bunkr").enabled);
    assert!(config.preprocess_for("gofile").enabled);
}

#[test]
fn clearing_a_service_override_restores_inheritance() {
    let mut config = Config::default();
    config.preprocess.enabled = true;
    config.preprocess.rules.push(PreprocessRule::new(
        PreprocessStrategy::by_id("wrap_zip").unwrap(),
    ));
    config.preprocess_mut("bunkr").unwrap().enabled = false;
    assert!(!config.preprocess_for("bunkr").enabled);

    assert!(config.clear_preprocess_override("bunkr"));
    assert!(config.preprocess_for("bunkr").enabled);
    assert!(!config.clear_preprocess_override("bunkr"));
}

#[test]
fn rules_parse_strategy_and_params() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let toml_str = r#"
        [bunkr.preprocess]
        enabled = true

        [[bunkr.preprocess.rules]]
        strategy = "compress_image"
        params = { format = "jpeg", quality = 70, max_dimensions = "1600x1600" }

        [[bunkr.preprocess.rules]]
        strategy = "normalize_name"
        params = { replacement = "_", lowercase = true }
    "#;
    std::fs::File::create(&path)
        .unwrap()
        .write_all(toml_str.as_bytes())
        .unwrap();

    let config = Config::from_file(&path).unwrap();
    let bunkr = config.bunkr.expect("bunkr parsed");
    let preprocess = bunkr.preprocess.expect("preprocess parsed");

    assert!(preprocess.enabled);
    assert_eq!(preprocess.rules.len(), 2);

    let image = &preprocess.rules[0];
    assert_eq!(image.strategy.def().id, "compress_image");
    assert_eq!(image.params["format"], "jpeg");
    assert_eq!(image.params["quality"], 70);

    let normalize = &preprocess.rules[1];
    assert_eq!(normalize.strategy.def().id, "normalize_name");
    assert_eq!(normalize.params["lowercase"], true);
}

#[test]
fn config_key_parse_display_roundtrip() {
    for &key in ConfigKey::ALL {
        let parsed: ConfigKey = key.as_str().parse().unwrap();
        assert_eq!(parsed, key);
        assert_eq!(key.to_string(), key.as_str());
    }
    assert!("bogus.key".parse::<ConfigKey>().is_err());
}

#[test]
fn settings_missing_file_yields_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.toml");

    let settings = Settings::load_from(&path).unwrap();
    let config = settings.extract().unwrap();

    assert!(config.bunkr.is_none());
    assert!(config.global.disabled_uploaders.is_empty());
}

#[test]
fn settings_layers_file_then_env_then_override() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[bunkr]\ntoken = \"from-file\"\n").unwrap();

    std::env::set_var("UPIO_GOFILE_TOKEN", "from-env");

    let mut settings = Settings::load_from(&path).unwrap();
    let config = settings.extract().unwrap();
    assert_eq!(config.bunkr.unwrap().token.as_deref(), Some("from-file"));
    assert_eq!(config.gofile.unwrap().token.as_deref(), Some("from-env"));

    // Explicit overrides win over env.
    settings.override_str("gofile.token", "from-override");
    let config = settings.extract().unwrap();
    assert_eq!(
        config.gofile.unwrap().token.as_deref(),
        Some("from-override")
    );

    std::env::remove_var("UPIO_GOFILE_TOKEN");
}

#[test]
fn settings_extract_inner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[bunkr]\ntoken = \"abc\"\n").unwrap();

    let settings = Settings::load_from(&path).unwrap();
    assert_eq!(
        settings
            .extract_inner::<String>("bunkr.token")
            .unwrap()
            .as_deref(),
        Some("abc")
    );
    assert_eq!(
        settings.extract_inner::<String>("bunkr.folder_id").unwrap(),
        None
    );
    assert_eq!(
        settings.extract_inner::<String>("missing.key").unwrap(),
        None
    );
}

#[test]
fn save_strips_null_params() {
    let config = Config {
        bunkr: Some(upio_core::UploaderEndpointConfig {
            preprocess: Some(PreprocessConfig {
                enabled: true,
                rules: vec![PreprocessRule {
                    // compress_image defaults contain `format: null`, which
                    // TOML cannot represent.
                    strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
                    params: serde_json::json!({ "format": null, "quality": 85 }),
                }],
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let toml = config.to_toml().expect("nulls must be stripped on save");
    assert!(
        !toml.contains("null"),
        "serialized config must not contain nulls:\n{toml}"
    );

    let reparsed: Config = toml::from_str(&toml).unwrap();
    let preprocess = reparsed
        .bunkr
        .unwrap()
        .preprocess
        .expect("override survives the roundtrip");
    let rule = &preprocess.rules[0];
    assert_eq!(rule.strategy.def().id, "compress_image");
    assert_eq!(rule.params["quality"], 85);
    assert!(rule.params.get("format").is_none());
}

#[test]
fn write_atomic_replaces_file_completely() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.toml");
    std::fs::write(&path, "original = true\n").unwrap();

    upio_config::write_atomic(&path, b"replaced = true\n").unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, "replaced = true\n");

    // Creating the parent directory implicitly must work too.
    let nested = dir.path().join("a/b/out.toml");
    upio_config::write_atomic(&nested, b"x = 1\n").unwrap();
    assert_eq!(std::fs::read_to_string(&nested).unwrap(), "x = 1\n");
}

#[test]
fn to_file_roundtrips_through_atomic_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    let mut config = upio_config::Config::default();
    config
        .set_value(&upio_config::ConfigKey::BunkrToken, "abc")
        .unwrap();
    config.to_file(&path).unwrap();

    let loaded = upio_config::Config::from_file(&path).unwrap();
    assert_eq!(loaded.get_value(&upio_config::ConfigKey::BunkrToken), "abc");
}

#[test]
fn is_uploader_enabled_matches_exact_names_only() {
    let disabled = vec![
        "fileditch".to_string(),
        "".to_string(),
        "bunkr extra".to_string(),
    ];
    assert!(!upio_config::is_uploader_enabled(&disabled, "fileditch"));
    assert!(upio_config::is_uploader_enabled(&disabled, "gofile"));
    // Matching is exact: an empty disabled entry only disables an empty
    // (never a real) service name, and partial names never match.
    assert!(!upio_config::is_uploader_enabled(&disabled, ""));
    assert!(upio_config::is_uploader_enabled(&disabled, "bunkr"));
    assert!(upio_config::is_uploader_enabled(
        &["bunk".to_string()],
        "bunkr"
    ));
}

#[test]
fn fileditch_token_key_is_a_secret_credential() {
    let key = "fileditch.token"
        .parse::<upio_config::ConfigKey>()
        .expect("fileditch.token is a config key");
    assert!(ConfigKey::ALL.contains(&key));
    assert_eq!(key.as_str(), "fileditch.token");
    assert!(key.is_secret());

    let mut config = Config::default();
    config.set_value(&key, "login-token").unwrap();
    assert_eq!(
        config.fileditch.as_ref().unwrap().token.as_deref(),
        Some("login-token")
    );
    assert_eq!(config.get_value(&key), "login-token");
}

#[test]
fn download_settings_have_working_defaults() {
    let config = Config::default();
    assert!(config.global.download.output_dir.is_none());
    assert_eq!(config.global.download.concurrency, 4);
    assert!(config.global.download.resume);
    assert_eq!(
        config.global.download.on_conflict,
        upio_core::download::ConflictPolicy::Rename
    );
    assert!(config.global.disabled_downloaders.is_empty());
}

#[test]
fn download_settings_parse_from_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let toml_str = r#"
        [global]
        disabled_downloaders = ["bunkr", "gofile"]

        [global.download]
        output_dir = "/tmp/upio"
        concurrency = 2
        resume = false
        on_conflict = "skip"
    "#;
    std::fs::write(&path, toml_str).unwrap();

    let config = Config::from_file(&path).unwrap();
    assert_eq!(
        config.global.disabled_downloaders,
        vec!["bunkr".to_string(), "gofile".to_string()]
    );
    assert_eq!(
        config.global.download.output_dir.as_deref(),
        Some(std::path::Path::new("/tmp/upio"))
    );
    assert_eq!(config.global.download.concurrency, 2);
    assert!(!config.global.download.resume);
    assert_eq!(
        config.global.download.on_conflict,
        upio_core::download::ConflictPolicy::Skip
    );
}

#[test]
fn download_config_keys_roundtrip() {
    let mut config = Config::default();

    config
        .set_value(&ConfigKey::GlobalDisabledDownloaders, "bunkr, gofile")
        .unwrap();
    assert_eq!(
        config.get_value(&ConfigKey::GlobalDisabledDownloaders),
        "bunkr, gofile"
    );

    config
        .set_value(&ConfigKey::GlobalDownloadOutputDir, "/tmp/upio")
        .unwrap();
    assert_eq!(
        config.get_value(&ConfigKey::GlobalDownloadOutputDir),
        "/tmp/upio"
    );

    config
        .set_value(&ConfigKey::GlobalDownloadConcurrency, "8")
        .unwrap();
    assert_eq!(config.get_value(&ConfigKey::GlobalDownloadConcurrency), "8");

    config
        .set_value(&ConfigKey::GlobalDownloadResume, "false")
        .unwrap();
    assert_eq!(config.get_value(&ConfigKey::GlobalDownloadResume), "false");

    config
        .set_value(&ConfigKey::GlobalDownloadOnConflict, "overwrite")
        .unwrap();
    assert_eq!(
        config.get_value(&ConfigKey::GlobalDownloadOnConflict),
        "overwrite"
    );
}

#[test]
fn invalid_download_config_values_are_errors() {
    let mut config = Config::default();

    assert!(config
        .set_value(&ConfigKey::GlobalDownloadConcurrency, "0")
        .is_err());
    assert!(config
        .set_value(&ConfigKey::GlobalDownloadConcurrency, "many")
        .is_err());
    assert!(config
        .set_value(&ConfigKey::GlobalDownloadResume, "sometimes")
        .is_err());
    assert!(config
        .set_value(&ConfigKey::GlobalDownloadOnConflict, "merge")
        .is_err());
}

#[test]
fn is_downloader_enabled_matches_exact_names_only() {
    let disabled = vec!["bunkr".to_string(), "".to_string()];
    assert!(!upio_config::is_downloader_enabled(&disabled, "bunkr"));
    assert!(upio_config::is_downloader_enabled(&disabled, "gofile"));
    assert!(upio_config::is_downloader_enabled(
        &["bunk".to_string()],
        "bunkr"
    ));
}
