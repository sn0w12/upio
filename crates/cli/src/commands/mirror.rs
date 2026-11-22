use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use futures::StreamExt;
use tempfile::TempDir;

use upio::download::{self, DownloadOptions, DownloadPlan, DownloadProgress, PlannedFile};
use upio::registry::UploaderId;
use upio_config::{is_downloader_enabled, Config, Settings};

use crate::cli::{MirrorArgs, UploadArgs};
use crate::commands::download::{build_service, short_label, ServiceDownloader};
use crate::live::{Handle, Live};
use crate::output::{self, DownloadBatch, DownloadView, Item, Outcome, Phase};
use crate::upload::upload_to_uploaders;

/// Run the `mirror` command: download each share, then upload all of its files.
pub async fn run(args: MirrorArgs, config_path: Option<&Path>) -> Result<()> {
    if args.urls.is_empty() {
        bail!("No URLs to download");
    }

    let settings = Settings::load_from_or_default(config_path)?;
    let config = settings.extract()?;

    let concurrency = args
        .concurrency
        .unwrap_or(config.global.download.concurrency);
    if concurrency == 0 {
        bail!("concurrency must be at least 1");
    }

    let staging = Staging::new(&args, &config)?;
    let options = DownloadOptions {
        output_dir: staging.path.clone(),
        flat: false,
        resume: !args.no_resume && config.global.download.resume,
        on_conflict: config.global.download.on_conflict,
    };

    let batches = args
        .urls
        .iter()
        .map(|url| match UploaderId::from_url(url) {
            Some(id) => DownloadBatch::resolving(short_label(id.def().display_name, url)),
            None => DownloadBatch::resolving(short_label("unknown", url)),
        })
        .collect::<Vec<_>>();

    let live = Live::start(DownloadView { batches }, |view, screen| {
        output::download_lines(&view.batches, screen, true)
    });

    let mut services: HashMap<&'static str, ServiceDownloader> = HashMap::new();
    let mut shares: Vec<SharePlan> = Vec::new();
    let mut taken = HashSet::new();

    for (index, url) in args.urls.iter().enumerate() {
        let Some(id) = UploaderId::from_url(url) else {
            fail_batch(&live, index, "no downloader supports this URL");
            continue;
        };

        if !is_downloader_enabled(&config.global.disabled_downloaders, id.name()) {
            fail_batch(&live, index, "disabled downloader");
            continue;
        }

        if !services.contains_key(id.name()) {
            match build_service(id, &config, args.password.as_deref()) {
                Ok(service) => {
                    services.insert(id.name(), service);
                }
                Err(error) => {
                    fail_batch(&live, index, error.to_string());
                    continue;
                }
            }
        }

        let service = services
            .get(id.name())
            .expect("the service was built above");
        let node = match service.downloader.resolve(url, &service.endpoint).await {
            Ok(node) => node,
            Err(error) => {
                fail_batch(&live, index, error.to_string());
                continue;
            }
        };
        let plan = match download::plan_with_taken(&node, &options, &mut taken) {
            Ok(plan) => plan,
            Err(error) => {
                fail_batch(&live, index, error.to_string());
                continue;
            }
        };
        if plan.is_empty() {
            fail_batch(&live, index, "no files");
            continue;
        }

        live.update(|view| {
            let batch = &mut view.batches[index];
            batch.phase = Phase::Running;
            batch.items = plan.files.iter().map(item_for).collect();
        });
        shares.push(SharePlan {
            batch: index,
            plan,
            service: id.name(),
        });
    }

    if shares.is_empty() {
        let view = live.view();
        live.finish();
        output::print_mirror(&view.batches, &[]);
        bail!("no files were downloaded");
    }

    let mut groups: BTreeMap<PathBuf, Vec<Job>> = BTreeMap::new();
    for share in &shares {
        for (item, file) in share.plan.files.iter().enumerate() {
            if file.skipped {
                continue;
            }
            groups
                .entry(path_key(&file.destination))
                .or_default()
                .push(Job {
                    batch: share.batch,
                    item,
                    file: file.clone(),
                    service: share.service,
                });
        }
    }

    let handle = live.handle();
    let resume = options.resume;
    let groups = groups.into_values().map(|jobs| {
        let handle = handle.clone();
        let services = &services;
        async move {
            for job in jobs {
                download_job(job, handle.clone(), services, resume).await;
            }
        }
    });
    futures::stream::iter(groups)
        .buffer_unordered(concurrency)
        .collect::<Vec<()>>()
        .await;

    live.update(|view| {
        for batch in &mut view.batches {
            if batch.phase == Phase::Running {
                batch.finish();
            }
        }
    });

    let view = live.view();
    live.finish();
    let batches = view.batches;

    let download_failed = batches
        .iter()
        .flat_map(|batch| &batch.items)
        .filter(|item| matches!(item.outcome, Outcome::Failed | Outcome::Notice))
        .count();

    let mut files = Vec::new();
    for batch in &batches {
        for item in &batch.items {
            if matches!(item.outcome, Outcome::Ok | Outcome::Skipped) {
                files.push(item.detail.clone());
            }
        }
    }
    if files.is_empty() {
        output::print_mirror(&batches, &[]);
        bail!("no files were downloaded");
    }

    let upload_args = UploadArgs {
        paths: Vec::new(),
        glob: Vec::new(),
        uploaders: args.uploaders,
        folder_id: args.folder_id,
        folder_name: args.folder_name,
        batch_size: args.batch_size,
    };

    let upload_results = match upload_to_uploaders(&files, &config, &upload_args).await {
        Ok(results) => results,
        Err(error) => {
            output::print_mirror(&batches, &[]);
            return Err(error);
        }
    };
    output::print_mirror(&batches, &upload_results);

    let upload_failed = upload_results
        .iter()
        .flat_map(|result| &result.items)
        .filter(|item| matches!(item.outcome, Outcome::Failed | Outcome::Notice))
        .count();
    let failed = download_failed + upload_failed;
    if failed > 0 {
        bail!("{failed} mirror operation(s) failed");
    }
    Ok(())
}

struct SharePlan {
    batch: usize,
    plan: DownloadPlan,
    service: &'static str,
}

struct Job {
    batch: usize,
    item: usize,
    file: PlannedFile,
    service: &'static str,
}

async fn download_job(
    job: Job,
    handle: Handle<DownloadView>,
    services: &HashMap<&'static str, ServiceDownloader>,
    resume: bool,
) {
    let service = services
        .get(job.service)
        .expect("the service was built above");
    let (batch, item) = (job.batch, job.item);

    let progress_handle = handle.clone();
    let last = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
    let progress = DownloadProgress::new(move |_destination, transferred, total| {
        let now = Instant::now();
        {
            let mut last = last.lock().unwrap_or_else(|poison| poison.into_inner());
            if now.duration_since(*last) < Duration::from_millis(80) {
                return;
            }
            *last = now;
        }
        progress_handle.update(|view| {
            let item = &mut view.batches[batch].items[item];
            if item.outcome == Outcome::Running {
                item.transferred = transferred;
                item.total = total;
            }
        });
    });

    handle.update(|view| {
        let item = &mut view.batches[batch].items[item];
        item.outcome = Outcome::Running;
        item.started = Some(Instant::now());
    });

    let result = download::download_file(
        &job.file.source,
        &job.file.destination,
        service.downloader.as_ref(),
        &service.endpoint,
        resume,
        Some(&progress),
    )
    .await;

    handle.update(|view| {
        let item = &mut view.batches[batch].items[item];
        match result {
            Ok(bytes) => item.done(bytes),
            Err(error) => item.fail(error.to_string()),
        }
    });
}

fn item_for(file: &PlannedFile) -> Item {
    let name = output::display_name(&file.destination);
    let destination = file.destination.display().to_string();
    if file.skipped {
        let bytes = std::fs::metadata(&file.destination)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        Item::skipped(name, destination, bytes)
    } else {
        Item::waiting(name, destination)
    }
}

fn fail_batch(live: &Live<DownloadView>, index: usize, message: impl Into<String>) {
    let message = message.into();
    live.update(|view| {
        let batch = &mut view.batches[index];
        batch.phase = Phase::Failed;
        batch.items = vec![Item::notice(message)];
    });
}

fn path_key(path: &Path) -> PathBuf {
    if cfg!(any(windows, target_os = "macos")) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

struct Staging {
    path: PathBuf,
    _temp: Option<TempDir>,
}

impl Staging {
    fn new(args: &MirrorArgs, config: &Config) -> Result<Self> {
        if let Some(path) = &args.staging {
            std::fs::create_dir_all(path)?;
            return Ok(Self {
                path: path.clone(),
                _temp: None,
            });
        }

        if args.keep {
            let path = config
                .global
                .download
                .output_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("."));
            std::fs::create_dir_all(&path)?;
            println!("staging: {}", path.display());
            return Ok(Self { path, _temp: None });
        }

        let temp = TempDir::new()?;
        let path = temp.path().to_path_buf();
        Ok(Self {
            path,
            _temp: Some(temp),
        })
    }
}
