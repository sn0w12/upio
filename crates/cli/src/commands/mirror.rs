use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use tempfile::TempDir;

use upio::download::DownloadOptions;
use upio::registry::UploaderId;
use upio_config::{is_downloader_enabled, Config, Settings};

use crate::cli::{MirrorArgs, UploadArgs};
use crate::commands::download::{build_service, download_service, short_label, ServiceDownloader};
use crate::output::{self, DownloadBatch, Outcome};
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

    let mut services: HashMap<&'static str, ServiceDownloader> = HashMap::new();
    let mut batches = Vec::with_capacity(args.urls.len());
    let mut files = Vec::new();

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
        let downloaded = download_service(label, url, service, &options, concurrency, false).await;
        files.extend(
            downloaded
                .files
                .iter()
                .map(|path| path.to_string_lossy().into_owned()),
        );
        batches.push(downloaded.batch);
    }

    let download_failed = batches
        .iter()
        .flat_map(|batch| &batch.items)
        .filter(|item| matches!(item.outcome, Outcome::Failed | Outcome::Notice))
        .count();

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
