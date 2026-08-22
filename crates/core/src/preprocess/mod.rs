//! Preprocessing of source files before upload.
//!
//! A preprocessing **strategy** is fully defined by its own module: its run
//! function, MIME patterns, exclusions, trigger, required tools, defaults and
//! parameter validation all live together in that file as a [`StrategyDef`].
//! [`STRATEGIES`] is the catalog of every strategy in the order they chain.
//!
//! Rules are configured per service through [`PreprocessConfig`]. Everything
//! except the parameters is owned by the rule's strategy; a rule present in
//! the config is enabled, and rules always apply in the canonical catalog
//! order regardless of how they are listed.

mod compress_image;
mod normalize_name;
mod split_video;
pub mod tools;
mod wrap_zip;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use self::tools::Tool;

/// Configuration for preprocessing files before upload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreprocessConfig {
    /// Master switch; when `false` no rules are applied.
    pub enabled: bool,
    /// The configured rules. A rule that is present is active; list order is
    /// irrelevant because strategies always chain in [`STRATEGIES`] order.
    pub rules: Vec<PreprocessRule>,
}

/// A single preprocessing rule: which strategy to run, with which parameters.
///
/// Everything else is owned by the rule's strategy itself (its MIME patterns,
/// trigger, required tools); a rule present in [`PreprocessConfig::rules`] is
/// enabled.
///
/// Serialization strips JSON `null`s from `params` (recursively), because
/// TOML — the config format — has no null representation.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PreprocessRule {
    /// What to do with matching files.
    pub strategy: PreprocessStrategy,
    /// Strategy-specific parameters; see the strategy's definition for the
    /// accepted keys and their defaults.
    #[serde(default = "default_params")]
    pub params: serde_json::Value,
}

impl PreprocessRule {
    /// A rule for `strategy` with its default parameters.
    pub fn new(strategy: PreprocessStrategy) -> Self {
        Self {
            strategy,
            params: (strategy.def().default_params)(),
        }
    }

    /// Recursively drop `null` entries so the params survive TOML round-trips.
    fn strip_nulls(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.retain(|_, v| !v.is_null());
                map.values_mut().for_each(Self::strip_nulls);
            }
            serde_json::Value::Array(items) => {
                items.iter_mut().for_each(Self::strip_nulls);
            }
            _ => {}
        }
    }
}

impl Serialize for PreprocessRule {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;

        let mut params = self.params.clone();
        Self::strip_nulls(&mut params);

        let mut state = serializer.serialize_struct("PreprocessRule", 2)?;
        state.serialize_field("strategy", &self.strategy)?;
        state.serialize_field("params", &params)?;
        state.end()
    }
}

fn default_params() -> serde_json::Value {
    serde_json::json!({})
}

/// When a strategy runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTrigger {
    /// Run whenever the file matches the strategy's patterns.
    Always,
    /// Run only when the file exceeds the size cap.
    IfOversized,
}

/// Everything that defines a preprocessing strategy. Each strategy's module
/// declares one of these as a `static DEF`.
pub struct StrategyDef {
    /// Stable identifier used in config files (`"split_video"`, ...).
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// One-line explanation of what the strategy does and what it needs.
    pub description: &'static str,
    /// MIME patterns the strategy applies to.
    pub match_types: &'static [&'static str],
    /// MIME patterns the strategy never applies to, even if they also match
    /// [`Self::match_types`].
    pub excluded_types: &'static [&'static str],
    /// When the strategy runs.
    pub trigger: RuleTrigger,
    /// External tools the strategy requires; a missing tool skips the rule.
    pub tools: &'static [Tool],
    pub has_options: bool,
    /// Whether this strategy exposes user-tunable parameters.
    /// Parameters written when a rule for this strategy is first created.
    pub default_params: fn() -> serde_json::Value,
    /// Validate a rule's `params`; called at config-check time.
    pub parse_params: fn(&serde_json::Value) -> Result<(), PreprocessError>,
    /// Transform one file into one or more outputs.
    ///
    /// `max_file_size` is the target service's size cap; `IfOversized`
    /// strategies may rely on having been triggered by an oversized input.
    pub run: fn(
        path: &str,
        max_file_size: u64,
        params: &serde_json::Value,
    ) -> Result<StrategyOutput, PreprocessError>,
}

/// A preprocessing strategy: a handle to its [`StrategyDef`].
#[derive(Clone, Copy)]
pub struct PreprocessStrategy(&'static StrategyDef);

impl PreprocessStrategy {
    /// The strategy's definition.
    pub const fn def(&self) -> &'static StrategyDef {
        self.0
    }

    /// Look up a strategy by its config identifier.
    pub fn by_id(id: &str) -> Option<Self> {
        STRATEGIES.iter().copied().find(|s| s.def().id == id)
    }
}

impl PartialEq for PreprocessStrategy {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl Eq for PreprocessStrategy {}

impl fmt::Debug for PreprocessStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.id)
    }
}

impl Serialize for PreprocessStrategy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.id)
    }
}

impl<'de> Deserialize<'de> for PreprocessStrategy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Self::by_id(&id)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown preprocess strategy '{id}'")))
    }
}

/// Every strategy, in the canonical chain order: rules always apply in this
/// sequence regardless of how they are listed in the config.
pub static STRATEGIES: &[PreprocessStrategy] = &[
    PreprocessStrategy(&normalize_name::DEF),
    PreprocessStrategy(&compress_image::DEF),
    PreprocessStrategy(&wrap_zip::DEF),
    PreprocessStrategy(&split_video::DEF),
];

fn position(strategy: PreprocessStrategy) -> usize {
    STRATEGIES
        .iter()
        .position(|known| *known == strategy)
        .unwrap_or(usize::MAX)
}

/// Errors produced while preprocessing a file.
#[derive(Debug)]
pub enum PreprocessError {
    Io(std::io::Error),
    ToolNotFound(String),
    ToolFailed { tool: &'static str, stderr: String },
    InvalidConfig(String),
    Process(String),
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreprocessError::Io(e) => write!(f, "io error: {}", e),
            PreprocessError::ToolNotFound(tool) => {
                write!(f, "required tool not available: {}", tool)
            }
            PreprocessError::ToolFailed { tool, stderr } => {
                write!(f, "{} failed: {}", tool, stderr)
            }
            PreprocessError::InvalidConfig(msg) => {
                write!(f, "invalid preprocess config: {}", msg)
            }
            PreprocessError::Process(msg) => write!(f, "processing failed: {}", msg),
        }
    }
}

impl std::error::Error for PreprocessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PreprocessError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PreprocessError {
    fn from(e: std::io::Error) -> Self {
        PreprocessError::Io(e)
    }
}

/// Result of preprocessing one file.
#[derive(Debug)]
pub struct PreprocessedFile {
    /// Paths to upload, in order.
    pub files: Vec<PathBuf>,
    /// How to clean up temporary artifacts after the upload.
    pub cleanup: CleanupPlan,
}

/// How to clean up artifacts created by preprocessing.
#[derive(Debug)]
pub enum CleanupPlan {
    /// Nothing was created; nothing to remove.
    None,
    /// Remove these scratch directories after upload.
    RemoveDirs(Vec<PathBuf>),
}

/// Preprocess a single file according to `config`.
///
/// Strategies whose patterns do not match the file, or whose exclusions do,
/// are skipped. Strategies run in the canonical [`STRATEGIES`] order and
/// chain: every applied strategy transforms the working set of files and
/// feeds its outputs into the next strategy.
///
/// ```
/// use upio_core::preprocess::{
///     cleanup, preprocess_file, PreprocessConfig, PreprocessRule, PreprocessStrategy,
/// };
///
/// # async fn run() {
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("my file.txt");
/// std::fs::write(&path, b"hello").unwrap();
///
/// let config = PreprocessConfig {
///     enabled: true,
///     rules: vec![PreprocessRule::new(PreprocessStrategy::by_id("normalize_name").unwrap())],
/// };
///
/// let prepped = preprocess_file(&path.to_string_lossy(), 1024, &config).await.unwrap();
/// assert_eq!(prepped.files[0].file_name().unwrap().to_string_lossy(), "my_file.txt");
///
/// cleanup(&prepped).unwrap();
/// # }
/// # let _ = run();
/// ```
pub async fn preprocess_file(
    path: &str,
    uploader_max_size: u64,
    config: &PreprocessConfig,
) -> Result<PreprocessedFile, PreprocessError> {
    if !config.enabled {
        return Ok(original(path));
    }

    // Rules always chain in the canonical strategy order, no matter how they
    // are listed in the config.
    let mut rules: Vec<&PreprocessRule> = config.rules.iter().collect();
    rules.sort_by_key(|rule| position(rule.strategy));

    let mut working = vec![PathBuf::from(path)];
    let mut temp_dirs: Vec<PathBuf> = Vec::new();

    for rule in rules {
        let mut next = Vec::new();
        for file in working {
            match apply_rule(&file, uploader_max_size, rule).await {
                Ok(Some(output)) => {
                    if let Some(dir) = output.temp_dir {
                        temp_dirs.push(dir);
                    }
                    next.extend(output.outputs);
                }
                Ok(None) => next.push(file),
                Err(e) => {
                    let _ = remove_temp_dirs(&temp_dirs);
                    return Err(e);
                }
            }
        }
        working = next;
    }

    let cleanup = if temp_dirs.is_empty() {
        CleanupPlan::None
    } else {
        CleanupPlan::RemoveDirs(temp_dirs)
    };

    Ok(PreprocessedFile {
        files: working,
        cleanup,
    })
}

/// Validate every configured rule's params, without transforming anything.
/// Useful for config-time checks (e.g. `upio-cli preprocess check`).
///
/// Returns an error describing the first invalid rule.
///
/// ```
/// use upio_core::preprocess::{
///     validate_config, PreprocessConfig, PreprocessRule, PreprocessStrategy,
/// };
///
/// let good = PreprocessConfig {
///     enabled: true,
///     rules: vec![PreprocessRule {
///         strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
///         params: serde_json::json!({ "format": "jpeg", "quality": 80 }),
///     }],
/// };
/// assert!(validate_config(&good).is_ok());
///
/// let bad = PreprocessConfig {
///     rules: vec![PreprocessRule {
///         strategy: PreprocessStrategy::by_id("compress_image").unwrap(),
///         params: serde_json::json!({ "format": "webp" }), // unsupported
///     }],
///     ..good.clone()
/// };
/// assert!(validate_config(&bad).is_err());
/// ```
pub fn validate_config(config: &PreprocessConfig) -> Result<(), PreprocessError> {
    for rule in &config.rules {
        (rule.strategy.def().parse_params)(&rule.params)?;
    }
    Ok(())
}

async fn apply_rule(
    file: &Path,
    uploader_max_size: u64,
    rule: &PreprocessRule,
) -> Result<Option<StrategyOutput>, PreprocessError> {
    let def = rule.strategy.def();

    let mime = mime_guess::from_path(file).first_or_octet_stream();
    let matches = def
        .match_types
        .iter()
        .any(|pattern| mime_matches(pattern, &mime));
    let excluded = def
        .excluded_types
        .iter()
        .any(|pattern| mime_matches(pattern, &mime));
    if !matches || excluded {
        return Ok(None);
    }

    // A missing tool skips the rule instead of failing the upload.
    for tool in def.tools {
        if !tool.available() {
            return Ok(None);
        }
    }

    match def.trigger {
        RuleTrigger::Always => {}
        RuleTrigger::IfOversized => {
            if uploader_max_size == 0 {
                return Err(PreprocessError::InvalidConfig(
                    "uploader max_file_size must be greater than zero".to_string(),
                ));
            }
            let size = fs::metadata(file).map_err(PreprocessError::from)?.len();
            if size <= uploader_max_size {
                return Ok(None);
            }
        }
    }

    let path = file.to_string_lossy().into_owned();
    let params = rule.params.clone();
    let run = def.run;

    let output = tokio::task::spawn_blocking(move || run(&path, uploader_max_size, &params))
        .await
        .map_err(|e| PreprocessError::Process(format!("preprocessing task failed: {}", e)))??;

    Ok(Some(output))
}

fn original(path: &str) -> PreprocessedFile {
    PreprocessedFile {
        files: vec![PathBuf::from(path)],
        cleanup: CleanupPlan::None,
    }
}

/// Remove temporary artifacts produced by preprocessing.
///
/// Safe to call on any [`PreprocessedFile`]; a [`CleanupPlan::None`] is a no-op.
/// Call this after the returned files have been uploaded. Every directory in
/// the plan is attempted even if an earlier one fails; the first failure is
/// returned so callers can surface it.
pub fn cleanup(preprocessed: &PreprocessedFile) -> Result<(), PreprocessError> {
    match &preprocessed.cleanup {
        CleanupPlan::None => Ok(()),
        CleanupPlan::RemoveDirs(dirs) => remove_temp_dirs(dirs),
    }
}

fn remove_temp_dirs(dirs: &[PathBuf]) -> Result<(), PreprocessError> {
    let mut first_error = None;
    for dir in dirs {
        // Chained strategies can create nested scratch dirs; removing an
        // ancestor already removes descendants, so a missing dir means the
        // goal (it is gone) is achieved.
        let result = fs::remove_dir_all(dir).or_else(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        });
        if let Err(e) = result {
            if first_error.is_none() {
                first_error = Some((dir.clone(), e));
            }
        }
    }
    match first_error {
        None => Ok(()),
        Some((dir, e)) => Err(PreprocessError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to remove temp dir '{}': {}", dir.display(), e),
        ))),
    }
}

fn mime_matches(pattern: &str, mime: &mime_guess::Mime) -> bool {
    let Some((pattern_type, pattern_subtype)) = pattern.split_once('/') else {
        return false;
    };
    let type_ok = pattern_type == "*" || pattern_type == mime.type_().as_str();
    let subtype_ok = pattern_subtype == "*" || pattern_subtype == mime.subtype().as_str();
    type_ok && subtype_ok
}

/// The outputs of a strategy, plus the scratch directory to clean up.
pub struct StrategyOutput {
    pub(crate) outputs: Vec<PathBuf>,
    pub(crate) temp_dir: Option<PathBuf>,
}

/// Parse a strategy's `params` JSON into a typed, validated struct.
pub(crate) fn parse_params<T>(
    value: &serde_json::Value,
    strategy: &str,
) -> Result<T, PreprocessError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let value = if value.is_null() {
        serde_json::json!({})
    } else {
        value.clone()
    };
    serde_json::from_value(value).map_err(|e| {
        PreprocessError::InvalidConfig(format!("invalid params for strategy '{}': {}", strategy, e))
    })
}

pub(crate) fn temp_dir_for(parent: &Path, label: &str) -> PathBuf {
    parent.join(format!(".{}_{}", label, random_id()))
}

fn random_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}-{:04x}", nanos, std::process::id())
}
