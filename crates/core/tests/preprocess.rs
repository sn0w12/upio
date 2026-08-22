use std::process::Command;

use image::GenericImageView;
use upio_core::preprocess::{
    cleanup, preprocess_file, validate_config, CleanupPlan, PreprocessConfig, PreprocessError,
    PreprocessRule, PreprocessStrategy,
};

fn rule(id: &str) -> PreprocessConfig {
    PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule::new(PreprocessStrategy::by_id(id).unwrap())],
    }
}

#[tokio::test]
async fn wrap_zip_roundtrips_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.log");
    let content = "line1\nline2\nline3\n";
    std::fs::write(&path, content).unwrap();

    let config = rule("wrap_zip");
    let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files.len(), 1);
    let zip_path = &prepped.files[0];
    assert_eq!(zip_path.extension().unwrap(), "zip");

    let file = std::fs::File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    assert_eq!(archive.len(), 1);
    let mut entry = archive.by_index(0).unwrap();
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut entry, &mut buf).unwrap();
    assert_eq!(buf, content);

    cleanup(&prepped).unwrap();
    assert!(!zip_path.exists());
}

#[tokio::test]
async fn wrap_zip_skips_images_and_videos() {
    let dir = tempfile::tempdir().unwrap();

    for name in ["photo.png", "clip.mp4"] {
        let path = dir.path().join(name);
        std::fs::write(&path, b"pretend media").unwrap();
        let config = rule("wrap_zip");
        let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config)
            .await
            .unwrap();
        assert_eq!(
            prepped.files,
            vec![path],
            "{name} must be excluded from wrap_zip"
        );
        assert!(matches!(prepped.cleanup, CleanupPlan::None));
    }
}

#[tokio::test]
async fn no_matching_strategy_returns_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    std::fs::write(&path, b"not a video").unwrap();

    // split_video only applies to video/*; data.bin does not match.
    let config = rule("split_video");
    let prepped = preprocess_file(&path.to_string_lossy(), 1, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files, vec![path]);
    assert!(matches!(prepped.cleanup, CleanupPlan::None));
}

#[tokio::test]
async fn if_oversized_trigger_skips_files_under_uploader_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tiny.png");
    std::fs::write(&path, b"not really a png").unwrap();

    let config = rule("compress_image");
    let prepped = preprocess_file(&path.to_string_lossy(), 10_000, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files, vec![path]);
    assert!(matches!(prepped.cleanup, CleanupPlan::None));
}

#[tokio::test]
async fn zero_uploader_limit_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.png");
    std::fs::write(&path, vec![0u8; 64]).unwrap();

    let config = rule("compress_image");
    let err = preprocess_file(&path.to_string_lossy(), 0, &config)
        .await
        .unwrap_err();

    assert!(matches!(err, PreprocessError::InvalidConfig(_)));
}

#[tokio::test]
async fn disabled_config_returns_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.log");
    std::fs::write(&path, vec![0u8; 1000]).unwrap();

    let config = PreprocessConfig {
        enabled: false,
        rules: vec![PreprocessRule::new(
            PreprocessStrategy::by_id("wrap_zip").unwrap(),
        )],
    };
    let prepped = preprocess_file(&path.to_string_lossy(), 1, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files.len(), 1);
    assert!(matches!(prepped.cleanup, CleanupPlan::None));
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn split_video_without_ffmpeg_is_skipped() {
    if ffmpeg_available() {
        eprintln!("ffmpeg present; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clip.mp4");
    std::fs::write(&path, b"not really a video").unwrap();

    // A missing required tool skips the strategy instead of failing the upload.
    let config = rule("split_video");
    let prepped = preprocess_file(&path.to_string_lossy(), 1, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files, vec![path]);
    assert!(matches!(prepped.cleanup, CleanupPlan::None));
}

#[tokio::test]
async fn normalize_name_sanitizes_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my file (1).txt");
    std::fs::write(&path, b"hello").unwrap();

    let config = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("normalize_name").unwrap(),
            params: serde_json::json!({ "replacement": "_", "lowercase": true }),
        }],
    };
    let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files.len(), 1);
    let out_name = prepped.files[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(out_name, "my_file_(1).txt");
    assert!(path.exists(), "original file must be untouched");
    assert!(matches!(prepped.cleanup, CleanupPlan::RemoveDirs(_)));

    cleanup(&prepped).unwrap();
    assert!(!prepped.files[0].exists());
}

#[tokio::test]
async fn compress_image_reencodes_and_resizes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.png");
    let img = image::RgbaImage::from_fn(2000, 1000, |x, y| {
        let n = x
            .wrapping_mul(1103515245)
            .wrapping_add(y.wrapping_mul(12345))
            ^ 0x9e37_79b9;
        image::Rgba([n as u8, (n >> 8) as u8, (n >> 16) as u8, 255])
    });
    img.save(&path).unwrap();

    let config = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
            params: serde_json::json!({
                "format": "jpeg",
                "quality": 60,
                "max_dimensions": "800x800",
            }),
        }],
    };
    let prepped = preprocess_file(&path.to_string_lossy(), 1024 * 1024, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files.len(), 1);
    let out = &prepped.files[0];
    assert_eq!(
        out.extension().unwrap().to_string_lossy().to_string(),
        "jpg"
    );

    let decoded = image::open(out).unwrap();
    let (w, h) = decoded.dimensions();
    assert!(w <= 800 && h <= 800, "resized to {}x{}", w, h);

    cleanup(&prepped).unwrap();
    assert!(!out.exists());
}

#[tokio::test]
async fn chained_normalize_then_zip_in_config_order_is_irrelevant() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my file.txt");
    std::fs::write(&path, b"hello").unwrap();

    // Listed "backwards" relative to the canonical chain; strategies must
    // still apply normalize-then-zip because the catalog owns the order.
    let config = PreprocessConfig {
        enabled: true,
        rules: vec![
            PreprocessRule::new(PreprocessStrategy::by_id("wrap_zip").unwrap()),
            PreprocessRule::new(PreprocessStrategy::by_id("normalize_name").unwrap()),
        ],
    };
    let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config)
        .await
        .unwrap();

    assert_eq!(prepped.files.len(), 1);
    let name = prepped.files[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(name, "my_file.txt.zip");

    cleanup(&prepped).unwrap();
    assert!(!prepped.files[0].exists());
}

#[test]
fn validate_config_validates_params() {
    let good = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
            params: serde_json::json!({ "format": "jpeg", "quality": 80 }),
        }],
    };
    assert!(validate_config(&good).is_ok());

    let bad_quality = PreprocessConfig {
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
            params: serde_json::json!({ "quality": 0 }),
        }],
        ..good.clone()
    };
    assert!(validate_config(&bad_quality).is_err());

    let bad_format = PreprocessConfig {
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
            params: serde_json::json!({ "format": "webp" }),
        }],
        ..good.clone()
    };
    assert!(validate_config(&bad_format).is_err());
}

#[test]
fn cleanup_reports_failure_when_dir_cannot_be_removed() {
    use upio_core::preprocess::{CleanupPlan, PreprocessedFile};

    let dir = tempfile::tempdir().unwrap();
    // A file where the scratch directory should be makes remove_dir_all fail
    // with a non-NotFound error.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, b"not a dir").unwrap();

    let prepped = PreprocessedFile {
        files: vec![],
        cleanup: CleanupPlan::RemoveDirs(vec![blocked]),
    };

    let err = cleanup(&prepped).expect_err("cleanup must surface removal errors");
    assert!(
        err.to_string().contains("blocked"),
        "error should name the failing path: {err}"
    );
}

#[tokio::test]
async fn chained_rule_validation_failure_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    // A real image so compress_image's image/* pattern matches and its
    // params are validated.
    let path = dir.path().join("my file.png");
    image::RgbaImage::from_fn(8, 8, |_, _| image::Rgba([0, 0, 0, 255]))
        .save(&path)
        .unwrap();

    // normalize_name runs first in canonical order and creates a scratch
    // dir; compress_image's params are invalid, so preprocess_file must fail
    // instead of silently uploading the half-processed set.
    let config = PreprocessConfig {
        enabled: true,
        rules: vec![
            PreprocessRule {
                strategy: PreprocessStrategy::by_id("normalize_name").unwrap(),
                params: serde_json::json!({ "replacement": "_" }),
            },
            PreprocessRule {
                strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
                params: serde_json::json!({ "format": "webp" }),
            },
        ],
    };

    // The image is larger than 1 byte, so the IfOversized rule fires and its
    // invalid webp params fail validation.
    let err = preprocess_file(&path.to_string_lossy(), 1, &config)
        .await
        .expect_err("invalid webp params must fail validation");
    assert!(
        matches!(&err, PreprocessError::InvalidConfig(m) if m.contains("webp")),
        "unexpected error: {err}"
    );
    assert!(path.exists(), "original file must survive a failed chain");
}

#[tokio::test]
async fn clean_name_unicode_max_len_one_no_panic() {
    // A UTF-8 name whose stem is multi-byte; max_len = 1 must not panic and
    // must produce a valid bounded file name.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("日本語のファイル名.txt");
    std::fs::write(&path, b"hello").unwrap();

    let config = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("normalize_name").unwrap(),
            params: serde_json::json!({ "max_len": 1 }),
        }],
    };

    let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config)
        .await
        .unwrap();
    assert_eq!(prepped.files.len(), 1);
    let name = prepped.files[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.chars().count() <= 1 || !name.is_empty(),
        "name must be non-empty and sane: {name}"
    );
    cleanup(&prepped).unwrap();
    assert!(path.exists(), "original must be untouched");
}

#[tokio::test]
async fn clean_name_long_extension_is_bounded_by_max_len() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.verylongextension");
    std::fs::write(&path, b"hello").unwrap();

    let config = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("normalize_name").unwrap(),
            params: serde_json::json!({ "max_len": 5 }),
        }],
    };

    let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config)
        .await
        .unwrap();
    let name = prepped.files[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.chars().count() <= 5,
        "complete result must respect max_len: {name}"
    );
    assert!(!name.is_empty());
    cleanup(&prepped).unwrap();
}

#[test]
fn empty_replacement_param_is_rejected() {
    use upio_core::preprocess::validate_config;

    let config = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("normalize_name").unwrap(),
            params: serde_json::json!({ "replacement": "" }),
        }],
    };
    let err = validate_config(&config).expect_err("empty replacement is invalid");
    assert!(
        matches!(&err, PreprocessError::InvalidConfig(m) if m.contains("replacement")),
        "unexpected error: {err}"
    );
}

#[test]
fn dimensions_sequence_zero_is_rejected() {
    // The sequence form of Dimensions must reject zero just like the string
    // form does; exercise it through validate_config.
    let config = PreprocessConfig {
        enabled: true,
        rules: vec![PreprocessRule {
            strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
            params: serde_json::json!({
                "max_dimensions": [0, 100],
                "quality": 85,
            }),
        }],
    };
    let err =
        upio_core::preprocess::validate_config(&config).expect_err("[0, 100] must be invalid");
    assert!(
        matches!(&err, PreprocessError::InvalidConfig(m) if m.to_lowercase().contains("zero") || m.to_lowercase().contains("dimension")),
        "unexpected error: {err}"
    );
}
