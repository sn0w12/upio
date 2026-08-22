//! `PreprocessStrategy::NormalizeName`: sanitize file names before upload.
//!
//! Produces a renamed copy (via hard link, falling back to copy) in a scratch
//! directory so the original file is never modified. Runs with the `always`
//! trigger typically.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{parse_params, temp_dir_for, PreprocessError, StrategyOutput};

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct NormalizeParams {
    /// Replacement for invalid characters and whitespace runs.
    pub replacement: String,
    /// Lowercase the resulting name.
    pub lowercase: bool,
    /// Maximum length of the resulting name (stem + extension).
    pub max_len: usize,
}

impl Default for NormalizeParams {
    fn default() -> Self {
        Self {
            replacement: "_".to_string(),
            lowercase: false,
            max_len: 255,
        }
    }
}

pub(crate) fn parse(value: &serde_json::Value) -> Result<NormalizeParams, PreprocessError> {
    let params = parse_params::<NormalizeParams>(value, "normalize_name")?;
    if params.max_len == 0 {
        return Err(PreprocessError::InvalidConfig(
            "normalize_name.max_len must be greater than zero".to_string(),
        ));
    }
    if params.replacement.is_empty() {
        return Err(PreprocessError::InvalidConfig(
            "normalize_name.replacement must not be empty".to_string(),
        ));
    }
    Ok(params)
}

pub(crate) fn normalize_name(
    path: &str,
    params_value: &serde_json::Value,
) -> Result<StrategyOutput, PreprocessError> {
    let params = parse(params_value)?;

    let source = Path::new(path);
    let file_name = source.file_name().ok_or_else(|| {
        PreprocessError::InvalidConfig(format!("cannot derive a file name from '{}'", path))
    })?;
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let temp_dir = temp_dir_for(parent, "normalize");
    fs::create_dir_all(&temp_dir)?;
    let output = temp_dir.join(clean_name(file_name, &params));
    let linked = fs::hard_link(source, &output).is_ok();
    if !linked && fs::copy(source, &output).is_err() {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(PreprocessError::Io(std::io::Error::other(format!(
            "failed to create a renamed copy of '{}'",
            path
        ))));
    }

    Ok(StrategyOutput {
        outputs: vec![output],
        temp_dir: Some(temp_dir),
    })
}

fn clean_name(file_name: &std::ffi::OsStr, params: &NormalizeParams) -> String {
    let name = file_name.to_string_lossy();
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    };

    let mut cleaned = sanitize(&stem, &params.replacement);
    if params.lowercase {
        cleaned = cleaned.to_lowercase();
    }

    // The cap applies to the complete result (stem + extension). Truncate the
    // stem on a char boundary; keep as much of the extension as fits.
    let ext_chars: Vec<char> = ext.chars().collect();
    let budget = params.max_len.saturating_sub(ext_chars.len());
    if cleaned.chars().count() > budget {
        cleaned = cleaned.chars().take(budget).collect();
    }
    if cleaned.is_empty() {
        // The extension alone meets or exceeds the cap: emit a deterministic
        // shortened name so the result is never extension-only.
        return "file".chars().take(params.max_len).collect();
    }

    let mut result = cleaned;
    for ch in ext_chars {
        if result.chars().count() + 1 > params.max_len {
            break;
        }
        result.push(ch);
    }
    if result.is_empty() {
        "file".to_string()
    } else {
        result
    }
}

fn sanitize(input: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending = false;
    for ch in input.chars() {
        let invalid = ch.is_control()
            || ch.is_whitespace()
            || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
        if invalid {
            if !pending {
                out.push_str(replacement);
                pending = true;
            }
        } else {
            out.push(ch);
            pending = false;
        }
    }
    if out.is_empty() {
        out.push_str(replacement);
    }
    out
}

/// The strategy definition: patterns, trigger, defaults and run function all
/// live here so the strategy is self-describing.
pub(crate) static DEF: super::StrategyDef = super::StrategyDef {
    id: "normalize_name",
    name: "Normalize name",
    description: "Sanitize file names (spaces, unicode, invalid characters); the original file is never modified.",
    match_types: &["*/*"],
    excluded_types: &[],
    trigger: super::RuleTrigger::Always,
    tools: &[],
    has_options: true,
    default_params: || serde_json::json!({
        "replacement": "_",
        "lowercase": false,
        "max_len": 255,
    }),
    parse_params: |value| parse(value).map(|_| ()),
    run: |path, _max_file_size, params| normalize_name(path, params),
};
