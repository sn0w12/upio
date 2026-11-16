use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use upio::download::{self, BoxedDownloader, ConflictPolicy, DownloadOptions};
use upio::registry::{self, UploaderId};
use upio::UploaderEndpointConfig;
use upio_config::{is_downloader_enabled, Settings};

use crate::cli::DownloadArgs;
use crate::output::{self, DownloadBatch, DownloadOutcome, DownloadRow};

struct ServiceDownloader {
    downloader: BoxedDownloader,
    endpoint: UploaderEndpointConfig,
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
            let mut endpoint = config.get_uploader_config(id.name());
            if let Some(password) = &args.password {
                endpoint.link_password = Some(password.clone());
            }
            match registry::build_downloader(id, &endpoint) {
                Ok(downloader) => {
                    services.insert(
                        id.name(),
                        ServiceDownloader {
                            downloader,
                            endpoint,
                        },
                    );
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
        batches.push(download_url(label, url, service, &options, concurrency, args.dry_run).await);
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

async fn download_url(
    label: String,
    url: &str,
    service: &ServiceDownloader,
    options: &DownloadOptions,
    concurrency: usize,
    dry_run: bool,
) -> DownloadBatch {
    let node = match service.downloader.resolve(url, &service.endpoint).await {
        Ok(node) => node,
        Err(error) => return failed(label, error.to_string()),
    };

    let plan = match download::plan(&node, options) {
        Ok(plan) => plan,
        Err(error) => return failed(label, error.to_string()),
    };
    if plan.is_empty() {
        return failed(label, "no files");
    }

    if dry_run {
        return DownloadBatch {
            label,
            rows: plan
                .files
                .iter()
                .map(|file| DownloadRow::planned(&file.destination))
                .collect(),
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
    DownloadBatch {
        label,
        rows: results.into_iter().map(DownloadRow::from_result).collect(),
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
