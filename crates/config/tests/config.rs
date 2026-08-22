use std::io::Write;

use upio_config::{Config, ConfigKey, Settings};

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
    assert!(bunkr.preprocess.enabled);
    assert_eq!(bunkr.preprocess.rules.len(), 2);

    let video = &bunkr.preprocess.rules[0];
    assert_eq!(video.strategy.def().id, "split_video");

    let zip = &bunkr.preprocess.rules[1];
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
    assert!(gofile.preprocess.enabled);
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

    assert!(bunkr.preprocess.enabled);
    assert_eq!(bunkr.preprocess.rules.len(), 2);

    let image = &bunkr.preprocess.rules[0];
    assert_eq!(image.strategy.def().id, "compress_image");
    assert_eq!(image.params["format"], "jpeg");
    assert_eq!(image.params["quality"], 70);

    let normalize = &bunkr.preprocess.rules[1];
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
    use upio_core::preprocess::{PreprocessConfig, PreprocessRule, PreprocessStrategy};

    let config = Config {
        bunkr: Some(upio_core::UploaderEndpointConfig {
            preprocess: PreprocessConfig {
                enabled: true,
                rules: vec![PreprocessRule {
                    // compress_image defaults contain `format: null`, which
                    // TOML cannot represent.
                    strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
                    params: serde_json::json!({ "format": null, "quality": 85 }),
                }],
            },
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
    let rule = &reparsed.bunkr.unwrap().preprocess.rules[0];
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
fn removed_fileditch_token_key_is_not_parseable() {
    assert!("fileditch.token".parse::<upio_config::ConfigKey>().is_err());
    // The FromStr arm is gone; ALL no longer contains it either.
    for key in upio_config::ConfigKey::ALL {
        assert_ne!(key.as_str(), "fileditch.token");
    }
}
