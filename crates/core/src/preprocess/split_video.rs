//! `PreprocessStrategy::SplitVideo`: split a video into independently playable
//! segments via `ffmpeg`/`ffprobe`.
//!
//! This strategy requires `ffmpeg` and `ffprobe` on `PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{temp_dir_for, PreprocessError, StrategyOutput};

/// Split `path` into playable segments whose estimated size is at most
/// `max_part_size` bytes.
pub(crate) fn split_video(
    path: &str,
    max_part_size: u64,
) -> Result<StrategyOutput, PreprocessError> {
    if max_part_size == 0 {
        return Err(PreprocessError::InvalidConfig(
            "split_video requires a max_part_size greater than zero".to_string(),
        ));
    }
    let source = Path::new(path);
    let stem = source
        .file_stem()
        .ok_or_else(|| {
            PreprocessError::InvalidConfig(format!("cannot derive a file name from '{}'", path))
        })?
        .to_string_lossy()
        .into_owned();
    let extension = source
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let temp_dir = temp_dir_for(parent, "split_video");
    fs::create_dir_all(&temp_dir)?;

    let duration = ffprobe_duration(path).inspect_err(|_| {
        let _ = fs::remove_dir_all(&temp_dir);
    })?;
    if !duration.is_finite() || duration <= 0.0 {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(PreprocessError::InvalidConfig(format!(
            "ffprobe reported an unusable duration for '{}': {}",
            path, duration
        )));
    }
    let size = fs::metadata(path)
        .inspect_err(|_| {
            let _ = fs::remove_dir_all(&temp_dir);
        })?
        .len();
    let part_count: u32 = (size / max_part_size)
        .checked_add(u64::from((size % max_part_size) != 0))
        .and_then(|count| u32::try_from(count).ok())
        // More parts than `u32` can name is effectively "unsplittable"; the
        // pattern below only renders %03d anyway.
        .filter(|count| *count <= 999)
        .ok_or_else(|| {
            let _ = fs::remove_dir_all(&temp_dir);
            PreprocessError::InvalidConfig(format!(
                "video '{}' would need too many parts at a {} byte cap",
                path, max_part_size
            ))
        })?;
    if part_count < 2 {
        let _ = fs::remove_dir_all(&temp_dir);
        return Ok(StrategyOutput {
            outputs: vec![PathBuf::from(path)],
            temp_dir: Some(temp_dir),
        });
    }
    let segment_time = duration / f64::from(part_count);

    let output_pattern = temp_dir.join(format!("{}_%03d.{}", stem, extension));

    let mut args: Vec<String> = Vec::new();
    if let Some(accel) = detect_hwaccel() {
        args.push("-hwaccel".to_string());
        args.push(accel);
    }
    args.push("-loglevel".to_string());
    args.push("quiet".to_string());
    args.push("-i".to_string());
    args.push(path.to_string());
    args.push("-f".to_string());
    args.push("segment".to_string());
    args.push("-segment_time".to_string());
    args.push(segment_time.to_string());
    args.push("-c".to_string());
    args.push("copy".to_string());
    args.push("-reset_timestamps".to_string());
    args.push("1".to_string());
    args.push(output_pattern.to_string_lossy().into_owned());

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .inspect_err(|_| {
            let _ = fs::remove_dir_all(&temp_dir);
        })
        .map_err(|e| PreprocessError::ToolNotFound(format!("ffmpeg: {}", e)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(PreprocessError::ToolFailed {
            tool: "ffmpeg",
            stderr,
        });
    }

    let mut parts = Vec::new();
    for i in 0..part_count {
        let part_path = temp_dir.join(format!("{}_{:03}.{}", stem, i, extension));
        if fs::metadata(&part_path).is_ok() {
            parts.push(part_path);
        }
    }

    Ok(StrategyOutput {
        outputs: parts,
        temp_dir: Some(temp_dir),
    })
}

fn ffprobe_duration(path: &str) -> Result<f64, PreprocessError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .output()
        .map_err(|e| PreprocessError::ToolNotFound(format!("ffprobe: {}", e)))?;
    if !output.status.success() {
        return Err(PreprocessError::ToolFailed {
            tool: "ffprobe",
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let stdout = String::from_utf8(output.stdout).map_err(|e| {
        PreprocessError::InvalidConfig(format!("ffprobe returned invalid UTF-8: {}", e))
    })?;
    stdout.trim().parse::<f64>().map_err(|e| {
        PreprocessError::InvalidConfig(format!(
            "could not parse duration from '{}': {}",
            stdout.trim(),
            e
        ))
    })
}

fn detect_hwaccel() -> Option<String> {
    let output = Command::new("ffmpeg").arg("-hwaccels").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let position = lines
        .iter()
        .position(|line| line.contains("Hardware acceleration methods:"))?;
    for line in lines.iter().skip(position + 1) {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed != "none" {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// The strategy definition: patterns, trigger, required tools, defaults and
/// run function all live here so the strategy is self-describing.
pub(crate) static DEF: super::StrategyDef = super::StrategyDef {
    id: "split_video",
    name: "Split video",
    description: "Split a video into independently playable segments via ffmpeg and ffprobe.",
    match_types: &["video/*"],
    excluded_types: &[],
    trigger: super::RuleTrigger::IfOversized,
    tools: &[super::tools::Tool::Ffmpeg, super::tools::Tool::Ffprobe],
    has_options: false,
    default_params: || serde_json::json!({}),
    parse_params: |_value| Ok(()),
    run: |path, max_file_size, _params| split_video(path, max_file_size),
};
