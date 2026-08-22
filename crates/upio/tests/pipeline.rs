use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use upio::filesize::FileSize;
use upio::pipeline::{upload_file, upload_files, FileResult};
use upio::preprocess::{PreprocessConfig, PreprocessRule, PreprocessStrategy};
use upio::{UploadError, UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};

#[derive(Clone)]
struct FakeUploader {
    max_file_size: FileSize,
    fail: bool,
    fail_init: bool,
    uploaded: Arc<Mutex<Vec<String>>>,
    /// Number of times [`Uploader::init`] ran; atomics need no locking.
    init_count: Arc<AtomicUsize>,
}

impl FakeUploader {
    fn new() -> Self {
        Self {
            max_file_size: FileSize::MAX,
            fail: false,
            fail_init: false,
            uploaded: Arc::new(Mutex::new(Vec::new())),
            init_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn uploaded_paths(&self) -> Vec<String> {
        self.uploaded.lock().unwrap().clone()
    }

    fn inits(&self) -> usize {
        self.init_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Uploader for FakeUploader {
    async fn init(&self) -> Result<(), UploadError> {
        self.init_count.fetch_add(1, Ordering::SeqCst);
        if self.fail_init {
            return Err(UploadError {
                message: "init failed".to_string(),
                status_code: None,
            });
        }
        Ok(())
    }

    async fn upload_file(
        &self,
        file_path: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<UploadResult, UploadError> {
        self.uploaded.lock().unwrap().push(file_path.to_string());
        if self.fail {
            return Err(UploadError {
                message: "upload failed".to_string(),
                status_code: None,
            });
        }
        Ok(UploadResult {
            urls: vec![format!("https://fake/{}", file_path.replace('\\', "/"))],
            raw_response: None,
        })
    }

    fn name(&self) -> &str {
        "fake"
    }

    fn capabilities(&self) -> UploaderCapabilities {
        UploaderCapabilities::any()
    }

    fn max_file_size(&self) -> FileSize {
        self.max_file_size
    }

    async fn is_ready(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn uploads_single_file_without_preprocessing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader::new();
    let config = UploaderEndpointConfig::default();

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert_eq!(result.file, path.to_string_lossy().to_string());
    assert_eq!(result.urls.len(), 1);
    assert!(result.error.is_none());
    assert_eq!(
        uploader.uploaded_paths(),
        vec![path.to_string_lossy().to_string()]
    );
}

#[tokio::test]
async fn preprocess_wraps_file_and_uploads_result() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.log");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader::new();
    let config = UploaderEndpointConfig {
        preprocess: PreprocessConfig {
            enabled: true,
            rules: vec![PreprocessRule::new(
                PreprocessStrategy::by_id("wrap_zip").unwrap(),
            )],
        },
        ..Default::default()
    };

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.is_none());
    assert_eq!(result.urls.len(), 1);
    assert_eq!(uploader.uploaded_paths().len(), 1);
    for uploaded in uploader.uploaded_paths() {
        assert!(
            !Path::new(&uploaded).exists(),
            "temporary zip was not cleaned up: {}",
            uploaded
        );
    }
}

#[tokio::test]
async fn upload_failure_surfaces_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader {
        fail: true,
        ..FakeUploader::new()
    };
    let config = UploaderEndpointConfig::default();

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.is_some());
    assert!(result.urls.is_empty());
}

#[tokio::test]
async fn partial_failure_reports_error_but_keeps_urls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.log");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader {
        fail: true,
        ..FakeUploader::new()
    };
    let config = UploaderEndpointConfig {
        preprocess: PreprocessConfig {
            enabled: true,
            rules: vec![PreprocessRule::new(
                PreprocessStrategy::by_id("wrap_zip").unwrap(),
            )],
        },
        ..Default::default()
    };

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.is_some());
    assert!(result.urls.is_empty());
}

#[tokio::test]
async fn upload_files_processes_each_file() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<String> = (0..4)
        .map(|i| {
            let p = dir.path().join(format!("f{}.txt", i));
            std::fs::write(&p, vec![i as u8; 10]).unwrap();
            p.to_string_lossy().to_string()
        })
        .collect();

    let uploader = FakeUploader::new();
    let config = UploaderEndpointConfig::default();

    let results: Vec<FileResult> = upload_files(&files, &uploader, &config, None).await;

    assert_eq!(results.len(), 4);
    assert!(results.iter().all(|r| r.error.is_none()));
    assert_eq!(results.iter().map(|r| r.urls.len()).sum::<usize>(), 4);
}

#[tokio::test]
async fn upload_files_with_concurrency_preserves_results() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<String> = (0..6)
        .map(|i| {
            let p = dir.path().join(format!("f{}.txt", i));
            std::fs::write(&p, vec![i as u8; 100]).unwrap();
            p.to_string_lossy().to_string()
        })
        .collect();

    let uploader = FakeUploader::new();
    let config = UploaderEndpointConfig::default();

    let results: Vec<FileResult> = upload_files(&files, &uploader, &config, Some(3)).await;

    assert_eq!(results.len(), 6);
    assert!(results.iter().all(|r| r.error.is_none()));
    assert_eq!(results.iter().map(|r| r.urls.len()).sum::<usize>(), 6);
}

#[tokio::test]
async fn chained_rules_upload_and_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my file.txt");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader::new();
    let config = UploaderEndpointConfig {
        preprocess: PreprocessConfig {
            enabled: true,
            rules: vec![
                PreprocessRule {
                    strategy: PreprocessStrategy::by_id("normalize_name").unwrap(),
                    params: serde_json::json!({ "replacement": "_" }),
                },
                PreprocessRule {
                    strategy: PreprocessStrategy::by_id("wrap_zip").unwrap(),
                    params: serde_json::json!({}),
                },
            ],
        },
        ..Default::default()
    };

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.is_none());
    assert_eq!(result.urls.len(), 1);
    let uploaded = uploader.uploaded_paths();
    assert_eq!(uploaded.len(), 1);
    let uploaded = &uploaded[0];
    assert!(
        uploaded.ends_with("my_file.txt.zip"),
        "unexpected uploaded path: {}",
        uploaded
    );
    assert!(
        !Path::new(uploaded).exists(),
        "zip was not cleaned up: {}",
        uploaded
    );
}

#[tokio::test]
async fn init_runs_without_preprocessing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader::new();
    let config = UploaderEndpointConfig::default();

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.is_none());
    assert_eq!(uploader.inits(), 1, "init must run on the plain path too");
    assert_eq!(uploader.uploaded_paths().len(), 1);
}

#[tokio::test]
async fn init_failure_performs_zero_uploads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader {
        fail_init: true,
        ..FakeUploader::new()
    };
    let config = UploaderEndpointConfig::default();

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.unwrap().starts_with("init failed:"));
    assert!(result.urls.is_empty());
    assert_eq!(uploader.inits(), 1);
    assert!(
        uploader.uploaded_paths().is_empty(),
        "an init failure must prevent every upload call"
    );
}

#[tokio::test]
async fn init_failure_with_preprocessing_also_blocks_uploads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.log");
    std::fs::write(&path, b"hello").unwrap();

    let uploader = FakeUploader {
        fail_init: true,
        ..FakeUploader::new()
    };
    let config = UploaderEndpointConfig {
        preprocess: PreprocessConfig {
            enabled: true,
            rules: vec![PreprocessRule::new(
                PreprocessStrategy::by_id("wrap_zip").unwrap(),
            )],
        },
        ..Default::default()
    };

    let result = upload_file(&path.to_string_lossy(), &uploader, &config).await;

    assert!(result.error.unwrap().contains("init failed"));
    assert!(uploader.uploaded_paths().is_empty());
}
