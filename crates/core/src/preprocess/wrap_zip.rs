//! `PreprocessStrategy::WrapZip`: wrap a file into a compressed `.zip`.
//!
//! Pure-Rust implementation using the `zip` crate. Good for text-heavy files
//! that compress well.

use std::fs::{self, File};
use std::path::Path;

use serde::Deserialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::{parse_params, temp_dir_for, PreprocessError, StrategyOutput};

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ZipParams {
    /// Deflate compression level 0-9.
    pub level: u16,
    /// Store uncompressed instead of deflating.
    pub store: bool,
}

impl Default for ZipParams {
    fn default() -> Self {
        Self {
            level: 6,
            store: false,
        }
    }
}

pub(crate) fn parse(value: &serde_json::Value) -> Result<ZipParams, PreprocessError> {
    let params = parse_params::<ZipParams>(value, "wrap_zip")?;
    if params.level > 9 {
        return Err(PreprocessError::InvalidConfig(
            "wrap_zip.level must be between 0 and 9".to_string(),
        ));
    }
    Ok(params)
}

pub(crate) fn wrap_zip(
    path: &str,
    params_value: &serde_json::Value,
) -> Result<StrategyOutput, PreprocessError> {
    let params = parse(params_value)?;

    let source = Path::new(path);
    let file_name = source.file_name().ok_or_else(|| {
        PreprocessError::InvalidConfig(format!("cannot derive a file name from '{}'", path))
    })?;
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let temp_dir = temp_dir_for(parent, "zip");
    fs::create_dir_all(&temp_dir)?;

    let zip_path = temp_dir.join(format!("{}.zip", file_name.to_string_lossy()));
    let file = File::create(&zip_path).map_err(|e| {
        let _ = fs::remove_dir_all(&temp_dir);
        PreprocessError::from(e)
    })?;
    let mut writer = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(if params.store {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        })
        .compression_level(if params.store {
            None
        } else {
            Some(params.level as i64)
        });

    if let Err(e) = writer
        .start_file(file_name.to_string_lossy(), options)
        .map_err(|e| PreprocessError::Process(format!("failed to add file to zip: {}", e)))
    {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(e);
    }

    let mut source_file = File::open(source).map_err(PreprocessError::from)?;
    if let Err(e) = std::io::copy(&mut source_file, &mut writer)
        .map_err(|e| PreprocessError::Process(format!("failed to add file to zip: {}", e)))
    {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(e);
    }

    if let Err(e) = writer
        .finish()
        .map_err(|e| PreprocessError::Process(format!("failed to finalize zip: {}", e)))
    {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(e);
    }

    Ok(StrategyOutput {
        outputs: vec![zip_path],
        temp_dir: Some(temp_dir),
    })
}

/// The strategy definition: patterns, trigger, defaults and run function all
/// live here so the strategy is self-describing. Images and videos are
/// excluded — they are already compressed formats, zipping them wastes time.
pub(crate) static DEF: super::StrategyDef = super::StrategyDef {
    id: "wrap_zip",
    name: "Wrap in zip",
    description: "Wrap the file into a compressed zip archive (useful for text and log files). Skips images and videos.",
    match_types: &["*/*"],
    excluded_types: &["image/*", "video/*"],
    trigger: super::RuleTrigger::Always,
    tools: &[],
    has_options: true,
    default_params: || serde_json::json!({
        "level": 6,
        "store": false,
    }),
    parse_params: |value| parse(value).map(|_| ()),
    run: |path, _max_file_size, params| wrap_zip(path, params),
};
