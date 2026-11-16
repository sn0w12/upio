use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use upio::download::{
    self, BoxedDownloader, ConflictPolicy, DownloadObserver, DownloadOptions, DownloadProgress,
};
use upio::registry::{self, UploaderId};
use upio::UploaderEndpointConfig;
use upio_config::{is_downloader_enabled, Config, Settings};

use crate::cli::DownloadArgs;
use crate::live::Live;
use crate::output::{self, DownloadBatch, DownloadView, Item, Outcome, Phase};

pub(crate) struct ServiceDownloader {
    downloader: BoxedDownloader,
    endpoint: UploaderEndpointConfig,
}

pub(crate) struct Downloaded {
    pub(crate) batch: DownloadBatch,
    pub(crate) files: Vec<PathBuf>,
}

pub(crate) fn build_service(
    id: UploaderId,
    config: &Config,
    password: Option<&str>,
) -> Result<ServiceDownloader, upio::BuildError> {
    let mut endpoint = config.get_uploader_config(id.name());
    if let Some(password) = password {
        endpoint.link_password = Some(password.to_string());
    }
    let downloader = registry::build_downloader(id, &endpoint)?;
    Ok(ServiceDownloader {
        downloader,
        endpoint,
    })
}

/// Run the `download` command: resolve each URL, plan its files, and transfer.
pub async fn run(args: DownloadArgs, config_path: Option<&Path>) -> Result<()> {
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

    let on_conflict = match args.on_conflict.as_deref() {
        Some(value) => value
            .parse::<ConflictPolicy>()
            .map_err(anyhow::Error::msg)?,
        None => config.global.download.on_conflict,
    };

    let options = DownloadOptions {
        output_dir: args
            .output
            .clone()
            .or_else(|| config.global.download.output_dir.clone())
            .unwrap_or_else(|| PathBuf::from(".")),
        flat: args.flat,
        resume: !args.no_resume && config.global.download.resume,
        on_conflict,
    };

    let mut services: HashMap<&'static str, ServiceDownloader> = HashMap::new();
    let mut batches = Vec::with_capacity(args.urls.len());

    for url in &args.urls {
        let Some(id) = UploaderId::from_url(url) else {
            batches.push(DownloadBatch::failed(
                short_label("unknown", url),
                "no downloader supports this URL",
            ));
            continue;
        };
        let label = short_label(id.def().display_name, url);

        if !is_downloader_enabled(&config.global.disabled_downloaders, id.name()) {
            batches.push(DownloadBatch::failed(label, "disabled downloader"));
            continue;
        }

        if !services.contains_key(id.name()) {
            match build_service(id, &config, args.password.as_deref()) {
                Ok(service) => {
                    services.insert(id.name(), service);
                }
                Err(error) => {
                    batches.push(DownloadBatch::failed(label, error.to_string()));
                    continue;
                }
            }
        }

        let service = services
            .get(id.name())
            .expect("the service was built above");
        batches.push(
            download_service(label, url, service, &options, concurrency, args.dry_run)
                .await
                .batch,
        );
    }

    output::print_download(&batches);

    let failed = batches
        .iter()
        .flat_map(|batch| &batch.items)
        .filter(|item| matches!(item.outcome, Outcome::Failed | Outcome::Notice))
        .count();
    if failed > 0 {
        bail!("{} download(s) failed", failed);
    }
    Ok(())
}

pub(crate) async fn download_service(
    label: String,
    url: &str,
    service: &ServiceDownloader,
    options: &DownloadOptions,
    concurrency: usize,
    dry_run: bool,
) -> Downloaded {
    let live = Live::start(
        DownloadView {
            batches: vec![DownloadBatch::resolving(label.clone())],
        },
        |view, screen| output::download_lines(&view.batches, screen, true),
    );

    let node = match service.downloader.resolve(url, &service.endpoint).await {
        Ok(node) => node,
        Err(error) => return failed(live, error.to_string()),
    };

    let plan = match download::plan(&node, options) {
        Ok(plan) => plan,
        Err(error) => return failed(live, error.to_string()),
    };
    if plan.is_empty() {
        return failed(live, "no files".to_string());
    }

    live.update(|view| {
        let batch = &mut view.batches[0];
        batch.phase = Phase::Running;
        batch.items = plan
            .files
            .iter()
            .map(|file| {
                let name = output::display_name(&file.destination);
                let destination = file.destination.display().to_string();
                if file.skipped {
                    let bytes = std::fs::metadata(&file.destination)
                        .map(|metadata| metadata.len())
                        .unwrap_or(0);
                    Item::skipped(name, destination, bytes)
                } else if dry_run {
                    let mut item = Item::planned(name, destination);
                    item.bytes = file.source.size.unwrap_or(0);
                    item.total = item.bytes;
                    item
                } else {
                    Item::waiting(name, destination)
                }
            })
            .collect();
    });

    if dry_run {
        live.update(|view| view.batches[0].finish());
        let batch = take_batch(live);
        return Downloaded {
            batch,
            files: Vec::new(),
        };
    }

    let index = Arc::new(
        plan.files
            .iter()
            .enumerate()
            .map(|(index, file)| (file.destination.clone(), index))
            .collect::<HashMap<_, _>>(),
    );

    let progress = progress_callback(live.handle(), Arc::clone(&index));
    let observer = observer(live.handle(), Arc::clone(&index));

    let results = download::download_observed(
        &plan,
        service.downloader.as_ref(),
        &service.endpoint,
        concurrency,
        Some(progress),
        Some(observer),
    )
    .await;

    live.update(|view| view.batches[0].finish());
    let batch = take_batch(live);

    let mut files = Vec::with_capacity(results.len());
    for result in &results {
        if result.error.is_none() {
            files.push(result.destination.clone());
        }
    }

    Downloaded { batch, files }
}

fn failed(live: Live<DownloadView>, message: String) -> Downloaded {
    live.update(|view| {
        let batch = &mut view.batches[0];
        batch.phase = Phase::Failed;
        batch.items = vec![Item::notice(message)];
    });
    Downloaded {
        batch: take_batch(live),
        files: Vec::new(),
    }
}

fn take_batch(live: Live<DownloadView>) -> DownloadBatch {
    let mut view = live.view();
    live.finish();
    view.batches.remove(0)
}

fn index_of(index: &HashMap<PathBuf, usize>, destination: &Path) -> Option<usize> {
    index.get(destination).copied()
}

fn progress_callback(
    live: crate::live::Handle<DownloadView>,
    index: Arc<HashMap<PathBuf, usize>>,
) -> DownloadProgress {
    let last = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
    DownloadProgress::new(move |destination, transferred, total| {
        let now = Instant::now();
        {
            let mut last = last.lock().unwrap_or_else(|poison| poison.into_inner());
            if now.duration_since(*last) < Duration::from_millis(80) {
                return;
            }
            *last = now;
        }
        let Some(item_index) = index_of(&index, destination) else {
            return;
        };
        live.update(|view| {
            let item = &mut view.batches[0].items[item_index];
            if item.outcome == Outcome::Running {
                item.transferred = transferred;
                item.total = total;
            }
        });
    })
}

fn observer(
    live: crate::live::Handle<DownloadView>,
    index: Arc<HashMap<PathBuf, usize>>,
) -> DownloadObserver {
    let started_handle = live.clone();
    let started_index = Arc::clone(&index);
    let finished_handle = live;
    let finished_index = index;

    DownloadObserver::new(
        move |file| {
            let Some(item_index) = index_of(&started_index, &file.destination) else {
                return;
            };
            started_handle.update(|view| {
                let item = &mut view.batches[0].items[item_index];
                item.outcome = Outcome::Running;
                item.started = Some(Instant::now());
            });
        },
        move |result| {
            let Some(item_index) = index_of(&finished_index, &result.destination) else {
                return;
            };
            finished_handle.update(|view| {
                let item = &mut view.batches[0].items[item_index];
                if let Some(error) = &result.error {
                    item.fail(error.clone());
                } else if result.skipped {
                    item.outcome = Outcome::Skipped;
                    item.bytes = result.bytes;
                    item.transferred = result.bytes;
                    item.total = result.bytes;
                } else {
                    item.done(result.bytes);
                }
            });
        },
    )
}

pub(crate) fn short_label(service: &str, url: &str) -> String {
    const MAX_PATH: usize = 40;

    let path = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split(['?', '#'])
        .next()
        .unwrap_or(url);
    let path = if path.chars().count() <= MAX_PATH {
        path.to_string()
    } else {
        format!("{}...", path.chars().take(MAX_PATH).collect::<String>())
    };
    format!("{}: {}", service, path)
}
