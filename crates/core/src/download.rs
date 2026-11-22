//! Downloading: the trait a service implements, and the shared transfer engine.
//!
//! A service only has to turn a share URL into a [`RemoteNode`] tree of direct
//! URLs. Everything after that (naming, conflicts, resume, retries, atomic
//! completion) is handled here, so every service gets the same behavior.
//!
//! The transfer engine and `DirectDownloader` are behind the `download`
//! feature. The trait and manifest stay available so the service catalog does
//! not depend on it.

#[cfg(feature = "download")]
use std::collections::HashSet;
#[cfg(feature = "download")]
use std::path::{Path, PathBuf};
#[cfg(feature = "download")]
use std::time::Duration;

use async_trait::async_trait;
#[cfg(feature = "download")]
use futures::StreamExt;
#[cfg(feature = "download")]
use tokio::io::AsyncWriteExt;

use crate::error::UpioError;
#[cfg(feature = "download")]
use crate::filename::{sanitize_component, unique_name};
#[cfg(feature = "download")]
use crate::http::map_io_error;
use crate::http::map_reqwest_error;
use crate::types::UploaderEndpointConfig;

#[cfg(feature = "download")]
mod direct;

#[cfg(feature = "download")]
pub use direct::DirectDownloader;

/// How many times a failed transfer is retried before the error is returned.
#[cfg(feature = "download")]
const RETRIES: u32 = 3;

/// What kind of resource a service URL points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    File,
    Collection,
}

/// The result of classifying a URL as belonging to a service.
///
/// Classification is pure: no network access and no credentials. The resolver
/// may re-parse the URL, so `id` is a hint for callers and diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedUrl {
    pub id: String,
    pub kind: ResourceKind,
}

/// One file reachable at a direct URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFile {
    pub name: String,
    pub size: Option<u64>,
    pub url: String,
    /// Additional request headers the service needs for this file, such as a
    /// referer or a cookie.
    pub headers: Vec<(String, String)>,
}

impl RemoteFile {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: None,
            url: url.into(),
            headers: Vec::new(),
        }
    }
}

/// A resolved share URL: either a single file or a folder of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteNode {
    File(RemoteFile),
    Folder {
        name: String,
        children: Vec<RemoteNode>,
    },
}

/// A service that can resolve share URLs and open their files.
///
/// Implementors only need [`resolve`](Downloader::resolve) plus a client;
/// the default [`open`](Downloader::open) performs a plain GET with optional
/// range resume. Override it when a service mints short-lived links or needs
/// per-request session state.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Downloader: Send + Sync {
    fn client(&self) -> &reqwest::Client;

    /// One-time, potentially network-bound setup. Called before resolving.
    async fn init(&self) -> Result<(), UpioError> {
        Ok(())
    }

    /// Turn `url` into a file or folder of files.
    async fn resolve(
        &self,
        url: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError>;

    /// Open a file, resuming at `offset` when it is greater than zero.
    async fn open(
        &self,
        file: &RemoteFile,
        offset: u64,
        _config: &UploaderEndpointConfig,
    ) -> Result<reqwest::Response, UpioError> {
        let mut request = self.client().get(&file.url);
        for (name, value) in &file.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        request.send().await.map_err(map_reqwest_error)
    }

    /// The service name, matching [`crate::ServiceDef::name`] when the service
    /// also uploads.
    fn name(&self) -> &str;
}

pub type BoxedDownloader = Box<dyn Downloader>;

/// Progress callback receiving `(destination, transferred, total)` bytes.
///
/// `total` is zero when the server did not report a size.
#[cfg(feature = "download")]
#[derive(Clone)]
pub struct DownloadProgress {
    inner: std::sync::Arc<ProgressCallback>,
}

#[cfg(feature = "download")]
type ProgressCallback = dyn Fn(&Path, u64, u64) + Send + Sync;
#[cfg(feature = "download")]
impl DownloadProgress {
    pub fn new(f: impl Fn(&Path, u64, u64) + Send + Sync + 'static) -> Self {
        Self {
            inner: std::sync::Arc::new(f),
        }
    }

    pub fn emit(&self, destination: &Path, transferred: u64, total: u64) {
        (self.inner)(destination, transferred, total);
    }
}

/// Per-file lifecycle hooks for callers that render transfers as they happen.
#[cfg(feature = "download")]
#[derive(Clone)]
pub struct DownloadObserver {
    started: std::sync::Arc<dyn Fn(&PlannedFile) + Send + Sync>,
    finished: std::sync::Arc<dyn Fn(&DownloadResult) + Send + Sync>,
}

#[cfg(feature = "download")]
impl DownloadObserver {
    pub fn new(
        started: impl Fn(&PlannedFile) + Send + Sync + 'static,
        finished: impl Fn(&DownloadResult) + Send + Sync + 'static,
    ) -> Self {
        Self {
            started: std::sync::Arc::new(started),
            finished: std::sync::Arc::new(finished),
        }
    }

    fn start(&self, file: &PlannedFile) {
        (self.started)(file);
    }

    fn finish(&self, result: &DownloadResult) {
        (self.finished)(result);
    }
}

/// What to do when a destination already exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// Keep both by appending ` (n)` to the new file.
    #[default]
    Rename,
    /// Leave the existing file and report the new one as skipped.
    Skip,
    /// Replace the existing file.
    Overwrite,
}

impl std::str::FromStr for ConflictPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rename" => Ok(Self::Rename),
            "skip" => Ok(Self::Skip),
            "overwrite" => Ok(Self::Overwrite),
            other => Err(format!(
                "unknown conflict policy '{}': use rename, skip, or overwrite",
                other
            )),
        }
    }
}

impl std::fmt::Display for ConflictPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Rename => "rename",
            Self::Skip => "skip",
            Self::Overwrite => "overwrite",
        })
    }
}

/// How a download batch writes files to disk.
#[cfg(feature = "download")]
#[derive(Clone, Debug)]
pub struct DownloadOptions {
    pub output_dir: PathBuf,
    pub flat: bool,
    pub resume: bool,
    pub on_conflict: ConflictPolicy,
}

#[cfg(feature = "download")]
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("."),
            flat: false,
            resume: true,
            on_conflict: ConflictPolicy::Rename,
        }
    }
}

/// One resolved file with its final destination.
#[cfg(feature = "download")]
#[derive(Clone, Debug)]
pub struct PlannedFile {
    pub source: RemoteFile,
    pub destination: PathBuf,
    pub skipped: bool,
}

/// The complete set of files a URL will produce.
#[cfg(feature = "download")]
#[derive(Clone, Debug)]
pub struct DownloadPlan {
    pub files: Vec<PlannedFile>,
    pub resume: bool,
}

#[cfg(feature = "download")]
impl Default for DownloadPlan {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            resume: true,
        }
    }
}

#[cfg(feature = "download")]
impl DownloadPlan {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Sum of the sizes the service reported, ignoring unknown sizes.
    pub fn total_known_bytes(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| !file.skipped)
            .filter_map(|file| file.source.size)
            .sum()
    }
}

/// The outcome of downloading one planned file.
#[cfg(feature = "download")]
#[derive(Clone, Debug)]
pub struct DownloadResult {
    pub source_url: String,
    pub destination: PathBuf,
    pub bytes: u64,
    pub skipped: bool,
    pub error: Option<String>,
}

/// Turn a resolved tree into concrete destination paths.
///
/// Remote names are sanitized component by component, so every destination
/// stays under `options.output_dir`. Name collisions are resolved according to
/// the conflict policy before any bytes are written.
#[cfg(feature = "download")]
pub fn plan(node: &RemoteNode, options: &DownloadOptions) -> Result<DownloadPlan, UpioError> {
    plan_with_taken(node, options, &mut HashSet::new())
}

/// Like [`plan`], claiming destinations in `taken` so several plans can share
/// one output directory without colliding.
#[cfg(feature = "download")]
pub fn plan_with_taken(
    node: &RemoteNode,
    options: &DownloadOptions,
    taken: &mut HashSet<PathBuf>,
) -> Result<DownloadPlan, UpioError> {
    let mut candidates = Vec::new();
    collect(node, Path::new(""), options.flat, &mut candidates);

    let mut files = Vec::with_capacity(candidates.len());

    for (relative, source) in candidates {
        let mut destination = options.output_dir.join(&relative);
        let exists = destination.exists() || taken.contains(&path_key(&destination));
        let skipped = options.on_conflict == ConflictPolicy::Skip && exists;

        if !skipped && options.on_conflict == ConflictPolicy::Rename {
            destination = unique_name(&destination, &|candidate| {
                taken.contains(&path_key(candidate)) || candidate.exists()
            });
        }

        taken.insert(path_key(&destination));
        files.push(PlannedFile {
            source,
            destination,
            skipped,
        });
    }

    Ok(DownloadPlan {
        files,
        resume: options.resume,
    })
}

/// Download every planned file, running up to `concurrency` at once.
#[cfg(feature = "download")]
pub async fn download(
    plan: &DownloadPlan,
    downloader: &dyn Downloader,
    config: &UploaderEndpointConfig,
    concurrency: usize,
    progress: Option<DownloadProgress>,
) -> Vec<DownloadResult> {
    download_observed(plan, downloader, config, concurrency, progress, None).await
}

/// Like [`download`], with hooks that fire as each file starts and finishes.
#[cfg(feature = "download")]
pub async fn download_observed(
    plan: &DownloadPlan,
    downloader: &dyn Downloader,
    config: &UploaderEndpointConfig,
    concurrency: usize,
    progress: Option<DownloadProgress>,
    observer: Option<DownloadObserver>,
) -> Vec<DownloadResult> {
    futures::stream::iter(plan.files.iter().cloned())
        .map(|file| {
            let progress = progress.clone();
            let observer = observer.clone();
            let resume = plan.resume;
            async move { download_one(&file, downloader, config, resume, progress, observer).await }
        })
        .buffered(concurrency.max(1))
        .collect()
        .await
}

/// Download one file to `destination`.
///
/// Partial data lives in `<destination>.part` until the transfer completes, so
/// an interrupted run can resume and a destination never contains a truncated
/// file. Failed transfers are retried from the current partial size.
#[cfg(feature = "download")]
pub async fn download_file(
    file: &RemoteFile,
    destination: &Path,
    downloader: &dyn Downloader,
    config: &UploaderEndpointConfig,
    resume: bool,
    progress: Option<&DownloadProgress>,
) -> Result<u64, UpioError> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(map_io_error)?;
    }

    let partial = partial_path(destination);
    let mut last_error = None;

    for attempt in 0..=RETRIES {
        match transfer_once(
            file,
            destination,
            &partial,
            downloader,
            config,
            resume,
            progress,
        )
        .await
        {
            Ok(bytes) => return Ok(bytes),
            Err(error) => {
                if attempt == RETRIES || !is_retryable(&error) {
                    return Err(error);
                }
                last_error = Some(error);
                tokio::time::sleep(backoff(attempt)).await;
            }
        }
    }

    Err(last_error.expect("the retry loop runs at least once"))
}

#[cfg(feature = "download")]
async fn transfer_once(
    file: &RemoteFile,
    destination: &Path,
    partial: &Path,
    downloader: &dyn Downloader,
    config: &UploaderEndpointConfig,
    resume: bool,
    progress: Option<&DownloadProgress>,
) -> Result<u64, UpioError> {
    let mut offset = if resume {
        tokio::fs::metadata(partial)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0)
    } else {
        0
    };

    if offset > 0 && file.size.is_some_and(|size| offset > size) {
        remove_partial(partial).await?;
        offset = 0;
    }

    let (response, offset) = loop {
        let response = downloader.open(file, offset, config).await?;
        let status = response.status();

        if offset > 0 && status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            remove_partial(partial).await?;
            offset = 0;
            continue;
        }

        // A server that ignores Range answers 200 with the whole body, so the
        // partial file has to be discarded rather than appended to.
        let offset = if offset > 0 && status == reqwest::StatusCode::OK {
            0
        } else {
            offset
        };
        break (response, offset);
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let body = body.trim();
        let detail = if body.is_empty() {
            String::new()
        } else {
            format!(": {}", truncate_chars(body, 200))
        };
        return Err(UpioError {
            message: format!("download failed with status {status}{detail}"),
            status_code: Some(status.as_u16()),
        });
    }

    let total = content_range_total(&response)
        .or_else(|| response.content_length().map(|length| length + offset))
        .or(file.size);

    let mut output = if offset > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(partial)
            .await
            .map_err(map_io_error)?
    } else {
        tokio::fs::File::create(partial)
            .await
            .map_err(map_io_error)?
    };

    let mut written = offset;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_reqwest_error)?;
        output.write_all(&chunk).await.map_err(map_io_error)?;
        written += chunk.len() as u64;
        if let Some(progress) = progress {
            progress.emit(destination, written, total.unwrap_or(0));
        }
    }
    output.flush().await.map_err(map_io_error)?;
    drop(output);

    if let Some(expected) = file.size {
        if written != expected {
            return Err(UpioError {
                message: format!(
                    "downloaded {written} bytes but the service reported {expected}; partial file kept"
                ),
                status_code: None,
            });
        }
    }

    tokio::fs::rename(partial, destination)
        .await
        .map_err(map_io_error)?;
    Ok(written)
}

#[cfg(feature = "download")]
async fn download_one(
    file: &PlannedFile,
    downloader: &dyn Downloader,
    config: &UploaderEndpointConfig,
    resume: bool,
    progress: Option<DownloadProgress>,
    observer: Option<DownloadObserver>,
) -> DownloadResult {
    let source_url = file.source.url.clone();
    let destination = file.destination.clone();

    if file.skipped {
        let bytes = std::fs::metadata(&destination)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let result = DownloadResult {
            source_url,
            destination,
            bytes,
            skipped: true,
            error: None,
        };
        if let Some(observer) = &observer {
            observer.finish(&result);
        }
        return result;
    }

    if let Some(observer) = &observer {
        observer.start(file);
    }

    let result = match download_file(
        &file.source,
        &destination,
        downloader,
        config,
        resume,
        progress.as_ref(),
    )
    .await
    {
        Ok(bytes) => DownloadResult {
            source_url,
            destination,
            bytes,
            skipped: false,
            error: None,
        },
        Err(error) => DownloadResult {
            source_url,
            destination,
            bytes: 0,
            skipped: false,
            error: Some(error.to_string()),
        },
    };

    if let Some(observer) = &observer {
        observer.finish(&result);
    }
    result
}

#[cfg(feature = "download")]
fn collect(node: &RemoteNode, base: &Path, flat: bool, out: &mut Vec<(PathBuf, RemoteFile)>) {
    match node {
        RemoteNode::File(file) => {
            let name = sanitize_component(&file.name, "download");
            out.push((base.join(name), file.clone()));
        }
        RemoteNode::Folder { name, children } => {
            let base = if flat {
                base.to_path_buf()
            } else {
                base.join(sanitize_component(name, "folder"))
            };
            for child in children {
                collect(child, &base, flat, out);
            }
        }
    }
}

#[cfg(feature = "download")]
fn partial_path(destination: &Path) -> PathBuf {
    let mut path = destination.as_os_str().to_os_string();
    path.push(".part");
    PathBuf::from(path)
}

#[cfg(feature = "download")]
fn path_key(path: &Path) -> PathBuf {
    #[cfg(any(windows, target_os = "macos"))]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        path.to_path_buf()
    }
}

#[cfg(feature = "download")]
async fn remove_partial(partial: &Path) -> Result<(), UpioError> {
    match tokio::fs::remove_file(partial).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(map_io_error(error)),
    }
}

#[cfg(feature = "download")]
fn content_range_total(response: &reqwest::Response) -> Option<u64> {
    let value = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    parse_content_range_total(value)
}

#[cfg(feature = "download")]
fn parse_content_range_total(value: &str) -> Option<u64> {
    let (range, total) = value.rsplit_once('/')?;
    if !range.trim_start().starts_with("bytes ") {
        return None;
    }
    total.trim().parse().ok()
}

#[cfg(feature = "download")]
fn is_retryable(error: &UpioError) -> bool {
    match error.status_code {
        None => true,
        Some(status) => status == 408 || status == 429 || (500..600).contains(&status),
    }
}

#[cfg(feature = "download")]
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt.min(5)))
}

#[cfg(feature = "download")]
fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "download")]
    fn file(name: &str, url: &str) -> RemoteNode {
        RemoteNode::File(RemoteFile::new(name, url))
    }

    #[cfg(feature = "download")]
    fn options(output_dir: &Path) -> DownloadOptions {
        DownloadOptions {
            output_dir: output_dir.to_path_buf(),
            ..DownloadOptions::default()
        }
    }

    #[cfg(feature = "download")]
    #[test]
    fn folders_are_preserved_in_the_destination() {
        let node = RemoteNode::Folder {
            name: "Album".to_string(),
            children: vec![file("clip.mp4", "https://example.test/clip")],
        };
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&node, &options(dir.path())).unwrap();

        assert_eq!(plan.len(), 1);
        assert_eq!(plan.files[0].destination, dir.path().join("Album/clip.mp4"));
    }

    #[cfg(feature = "download")]
    #[test]
    fn flat_drops_remote_folder_names() {
        let node = RemoteNode::Folder {
            name: "Album".to_string(),
            children: vec![RemoteNode::Folder {
                name: "Nested".to_string(),
                children: vec![file("clip.mp4", "https://example.test/clip")],
            }],
        };
        let dir = tempfile::tempdir().unwrap();
        let mut opts = options(dir.path());
        opts.flat = true;
        let plan = plan(&node, &opts).unwrap();

        assert_eq!(plan.files[0].destination, dir.path().join("clip.mp4"));
    }

    #[cfg(feature = "download")]
    #[test]
    fn hostile_remote_names_stay_inside_the_output_directory() {
        let node = RemoteNode::Folder {
            name: "../..".to_string(),
            children: vec![file("../../escape.txt", "https://example.test/x")],
        };
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&node, &options(dir.path())).unwrap();
        let destination = &plan.files[0].destination;

        assert!(destination.starts_with(dir.path()));
        assert!(destination
            .components()
            .all(|component| !matches!(component, std::path::Component::ParentDir)));
    }

    #[cfg(feature = "download")]
    #[test]
    fn colliding_names_are_renamed() {
        let node = RemoteNode::Folder {
            name: "Album".to_string(),
            children: vec![
                file("clip.mp4", "https://example.test/a"),
                file("clip.mp4", "https://example.test/b"),
            ],
        };
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&node, &options(dir.path())).unwrap();

        assert_eq!(plan.files[0].destination, dir.path().join("Album/clip.mp4"));
        assert_eq!(
            plan.files[1].destination,
            dir.path().join("Album/clip (1).mp4")
        );
    }

    #[cfg(feature = "download")]
    #[test]
    fn plans_can_share_one_destination_set() {
        let dir = tempfile::tempdir().unwrap();
        let mut taken = HashSet::new();
        let opts = options(dir.path());

        let first = plan_with_taken(
            &file("clip.mp4", "https://example.test/a"),
            &opts,
            &mut taken,
        )
        .unwrap();
        let second = plan_with_taken(
            &file("clip.mp4", "https://example.test/b"),
            &opts,
            &mut taken,
        )
        .unwrap();

        assert_eq!(first.files[0].destination, dir.path().join("clip.mp4"));
        assert_eq!(second.files[0].destination, dir.path().join("clip (1).mp4"));
    }

    #[cfg(feature = "download")]
    #[test]
    fn skip_policy_honors_destinations_claimed_by_another_plan() {
        let dir = tempfile::tempdir().unwrap();
        let mut taken = HashSet::new();
        let mut opts = options(dir.path());
        opts.on_conflict = ConflictPolicy::Skip;

        plan_with_taken(
            &file("clip.mp4", "https://example.test/a"),
            &opts,
            &mut taken,
        )
        .unwrap();
        let second = plan_with_taken(
            &file("clip.mp4", "https://example.test/b"),
            &opts,
            &mut taken,
        )
        .unwrap();

        assert!(second.files[0].skipped);
    }

    #[cfg(feature = "download")]
    #[test]
    fn skip_policy_marks_existing_files() {
        let node = file("clip.mp4", "https://example.test/clip");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("clip.mp4"), b"old").unwrap();

        let mut opts = options(dir.path());
        opts.on_conflict = ConflictPolicy::Skip;
        let plan = plan(&node, &opts).unwrap();

        assert!(plan.files[0].skipped);
        assert_eq!(plan.total_known_bytes(), 0);
    }

    #[cfg(feature = "download")]
    #[test]
    fn overwrite_policy_keeps_the_original_name() {
        let node = file("clip.mp4", "https://example.test/clip");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("clip.mp4"), b"old").unwrap();

        let mut opts = options(dir.path());
        opts.on_conflict = ConflictPolicy::Overwrite;
        let plan = plan(&node, &opts).unwrap();

        assert!(!plan.files[0].skipped);
        assert_eq!(plan.files[0].destination, dir.path().join("clip.mp4"));
    }

    #[test]
    fn conflict_policy_roundtrips_through_strings() {
        for policy in [
            ConflictPolicy::Rename,
            ConflictPolicy::Skip,
            ConflictPolicy::Overwrite,
        ] {
            assert_eq!(policy.to_string().parse::<ConflictPolicy>(), Ok(policy));
        }
        assert!("bogus".parse::<ConflictPolicy>().is_err());
    }

    #[cfg(feature = "download")]
    #[test]
    fn content_range_total_is_read_when_present() {
        assert_eq!(parse_content_range_total("bytes 100-199/1234"), Some(1234));
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
        assert_eq!(parse_content_range_total("garbage"), None);
    }

    #[cfg(feature = "download")]
    #[test]
    fn partial_files_are_suffixed() {
        assert_eq!(
            partial_path(Path::new("out/clip.mp4")),
            PathBuf::from("out/clip.mp4.part")
        );
    }

    #[cfg(feature = "download")]
    #[test]
    fn retryable_statuses_are_transient_only() {
        assert!(is_retryable(&UpioError {
            message: String::new(),
            status_code: None
        }));
        assert!(is_retryable(&UpioError {
            message: String::new(),
            status_code: Some(429)
        }));
        assert!(is_retryable(&UpioError {
            message: String::new(),
            status_code: Some(503)
        }));
        assert!(!is_retryable(&UpioError {
            message: String::new(),
            status_code: Some(403)
        }));
    }
}
