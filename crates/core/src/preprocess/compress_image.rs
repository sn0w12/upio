//! `PreprocessStrategy::CompressImage`: re-encode and/or resize images.
//!
//! Output formats `jpeg` and `png` are supported with the pure-Rust `image`
//! codec. WebP output is rejected with a clear error because the pure-Rust
//! codec only supports lossless WebP encoding, which would not reduce size;
//! convert to `jpeg` instead.

use std::fs;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{GenericImageView, ImageEncoder, ImageFormat, ImageReader};
use serde::Deserialize;

use super::{parse_params, temp_dir_for, PreprocessError, StrategyOutput};

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct CompressParams {
    /// Output format: `jpeg` | `png` (defaults to the input format if supported).
    pub format: Option<String>,
    /// JPEG quality 1-100.
    pub quality: u8,
    /// Maximum dimensions to scale down to, preserving aspect ratio.
    pub max_dimensions: Option<Dimensions>,
    /// Unused today: re-encoding already drops embedded metadata.
    pub strip_metadata: bool,
}

impl Default for CompressParams {
    fn default() -> Self {
        Self {
            format: None,
            quality: 85,
            max_dimensions: None,
            strip_metadata: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl<'de> Deserialize<'de> for Dimensions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DimensionsVisitor;

        impl<'de> serde::de::Visitor<'de> for DimensionsVisitor {
            type Value = Dimensions;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "dimensions as \"1920x1080\" or [1920, 1080]")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let lower = v.to_ascii_lowercase();
                let (w, h) = lower
                    .split_once('x')
                    .ok_or_else(|| E::custom(format!("invalid dimensions: {}", v)))?;
                let width = w
                    .trim()
                    .parse()
                    .map_err(|_| E::custom(format!("invalid width: {}", w)))?;
                let height = h
                    .trim()
                    .parse()
                    .map_err(|_| E::custom(format!("invalid height: {}", h)))?;
                if width == 0 || height == 0 {
                    return Err(E::custom("dimensions must be non-zero"));
                }
                Ok(Dimensions { width, height })
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let width: u32 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &"two-element array"))?;
                let height: u32 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &"two-element array"))?;
                if width == 0 || height == 0 {
                    return Err(serde::de::Error::custom("dimensions must be non-zero"));
                }
                Ok(Dimensions { width, height })
            }
        }

        deserializer.deserialize_any(DimensionsVisitor)
    }
}

pub(crate) fn parse(value: &serde_json::Value) -> Result<CompressParams, PreprocessError> {
    let params = parse_params::<CompressParams>(value, "compress_image")?;
    if params.quality == 0 || params.quality > 100 {
        return Err(PreprocessError::InvalidConfig(
            "compress_image.quality must be between 1 and 100".to_string(),
        ));
    }
    if let Some(format) = params.format.as_deref() {
        match format {
            "jpeg" | "jpg" | "png" => {}
            "webp" => {
                return Err(PreprocessError::InvalidConfig(
                    "compress_image.format 'webp' is not supported: the pure-Rust image codec only \
                     offers lossless WebP encoding, which does not reduce size. Use 'jpeg' or 'png'"
                        .to_string(),
                ));
            }
            other => {
                return Err(PreprocessError::InvalidConfig(format!(
                    "compress_image.format '{}' is not supported (use 'jpeg' or 'png')",
                    other
                )));
            }
        }
    }
    Ok(params)
}

pub(crate) fn compress_image(
    path: &str,
    params_value: &serde_json::Value,
) -> Result<StrategyOutput, PreprocessError> {
    let params = parse(params_value)?;

    let source = Path::new(path);
    let reader = ImageReader::open(source)
        .map_err(|e| PreprocessError::Process(format!("failed to open image '{}': {}", path, e)))?
        .with_guessed_format()
        .map_err(|e| {
            PreprocessError::Process(format!("failed to detect image format '{}': {}", path, e))
        })?;
    let input_format = reader.format().ok_or_else(|| {
        PreprocessError::Process(format!("could not detect image format for '{}'", path))
    })?;
    let mut img = reader.decode().map_err(|e| {
        PreprocessError::Process(format!("failed to decode image '{}': {}", path, e))
    })?;

    if let Some(dims) = params.max_dimensions {
        let (w, h) = img.dimensions();
        if w > dims.width || h > dims.height {
            let (nw, nh) = fit_dimensions(w, h, dims);
            img = img.resize(nw, nh, image::imageops::FilterType::Lanczos3);
        }
    }

    let out_format = resolve_format(params.format.as_deref(), input_format);

    let mut encoded: Vec<u8> = Vec::new();
    match out_format {
        ImageFormat::Jpeg => {
            let mut enc = JpegEncoder::new_with_quality(&mut encoded, params.quality);
            enc.encode_image(&img.to_rgb8())
                .map_err(|e| PreprocessError::Process(format!("failed to encode JPEG: {}", e)))?;
        }
        ImageFormat::Png => {
            let rgba = img.to_rgba8();
            let enc = PngEncoder::new(&mut encoded);
            enc.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| PreprocessError::Process(format!("failed to encode PNG: {}", e)))?;
        }
        other => {
            return Err(PreprocessError::Process(format!(
                "cannot encode image as '{:?}'",
                other
            )));
        }
    }

    let stem = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let ext = match out_format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Png => "png",
        _ => "img",
    };
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let temp_dir = temp_dir_for(parent, "image");
    fs::create_dir_all(&temp_dir)?;
    let output = temp_dir.join(format!("{}.{}", stem, ext));
    fs::write(&output, encoded).map_err(|e| {
        let _ = fs::remove_dir_all(&temp_dir);
        PreprocessError::from(e)
    })?;

    Ok(StrategyOutput {
        outputs: vec![output],
        temp_dir: Some(temp_dir),
    })
}

fn resolve_format(requested: Option<&str>, input: ImageFormat) -> ImageFormat {
    match requested {
        Some("jpeg") | Some("jpg") => ImageFormat::Jpeg,
        Some("png") => ImageFormat::Png,
        _ => match input {
            ImageFormat::Jpeg => ImageFormat::Jpeg,
            ImageFormat::Png => ImageFormat::Png,
            _ => ImageFormat::Jpeg,
        },
    }
}

fn fit_dimensions(w: u32, h: u32, cap: Dimensions) -> (u32, u32) {
    let w = w.max(1) as f64;
    let h = h.max(1) as f64;
    let scale = (cap.width as f64 / w).min(cap.height as f64 / h).min(1.0);
    let nw = (w * scale).round().max(1.0) as u32;
    let nh = (h * scale).round().max(1.0) as u32;
    (nw, nh)
}

/// The strategy definition: patterns, trigger, defaults and run function all
/// live here so the strategy is self-describing.
pub(crate) static DEF: super::StrategyDef = super::StrategyDef {
    id: "compress_image",
    name: "Compress image",
    description: "Re-encode and resize images so they fit the service limit.",
    match_types: &["image/*"],
    excluded_types: &[],
    trigger: super::RuleTrigger::IfOversized,
    tools: &[],
    has_options: true,
    default_params: || {
        serde_json::json!({
            "format": null,
            "quality": 85,
            "strip_metadata": true,
        })
    },
    parse_params: |value| parse(value).map(|_| ()),
    run: |path, _max_file_size, params| compress_image(path, params),
};
