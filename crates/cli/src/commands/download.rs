use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use upio::download::{self, BoxedDownloader, ConflictPolicy, DownloadOptions};
use upio::registry::{self, UploaderId};
use upio::UploaderEndpointConfig;
use upio_config::{is_downloader_enabled, Config, Settings};

use crate::cli::DownloadArgs;
use crate::output::{self, DownloadBatch, DownloadOutcome, DownloadRow};

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
            batches.push(failed(
                short_label("unknown", url),
                "no downloader supports this URL",
            ));
            continue;
        };
        let label = short_label(id.def().display_name, url);

        if !is_downloader_enabled(&config.global.disabled_downloaders, id.name()) {
            batches.push(failed(label, "disabled downloader"));
            continue;
        }

        if !services.contains_key(id.name()) {
            match build_service(id, &config, args.password.as_deref()) {
                Ok(service) => {
                    services.insert(id.name(), service);
                }
                Err(error) => {
                    batches.push(failed(label, error.to_string()));
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
        .flat_map(|batch| &batch.rows)
        .filter(|row| row.outcome == DownloadOutcome::Failed)
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
    let node = match service.downloader.resolve(url, &service.endpoint).await {
        Ok(node) => node,
        Err(error) => return Downloaded::failed(label, error.to_string()),
    };

    let plan = match download::plan(&node, options) {
        Ok(plan) => plan,
        Err(error) => return Downloaded::failed(label, error.to_string()),
    };
    if plan.is_empty() {
        return Downloaded::failed(label, "no files");
    }

    if dry_run {
        let rows = plan
            .files
            .iter()
            .map(|file| DownloadRow::planned(&file.destination))
            .collect();
        return Downloaded {
            batch: DownloadBatch { label, rows },
            files: Vec::new(),
        };
    }

    let results = download::download(
        &plan,
        service.downloader.as_ref(),
        &service.endpoint,
        concurrency,
        None,
    )
    .await;
    let mut files = Vec::with_capacity(results.len());
    let mut rows = Vec::with_capacity(results.len());
    for result in results {
        if result.error.is_none() {
            files.push(result.destination.clone());
        }
        rows.push(DownloadRow::from_result(result));
    }

    Downloaded {
        batch: DownloadBatch { label, rows },
        files,
    }
}

impl Downloaded {
    fn failed(label: String, status: impl Into<String>) -> Self {
        Self {
            batch: failed(label, status),
            files: Vec::new(),
        }
    }
}

fn failed(label: String, status: impl Into<String>) -> DownloadBatch {
    DownloadBatch {
        label,
        rows: vec![DownloadRow::failure(status)],
    }
}

fn short_label(service: &str, url: &str) -> String {
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
