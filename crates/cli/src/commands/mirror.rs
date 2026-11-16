use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use tempfile::TempDir;

use upio::download::DownloadOptions;
use upio::registry::UploaderId;
use upio_config::{is_downloader_enabled, Settings};

use crate::cli::{MirrorArgs, UploadArgs};
use crate::commands::download::{build_service, download_service};
use crate::output::{self, DownloadOutcome};
use crate::upload::upload_to_uploaders;

/// Run the `mirror` command: download one share, then upload its files.
pub async fn run(args: MirrorArgs, config_path: Option<&Path>) -> Result<()> {
    let settings = Settings::load_from_or_default(config_path)?;
    let config = settings.extract()?;

    let concurrency = args
        .concurrency
        .unwrap_or(config.global.download.concurrency);
    if concurrency == 0 {
        bail!("concurrency must be at least 1");
    }

    let Some(id) = UploaderId::from_url(&args.url) else {
        bail!("no downloader supports '{}'", args.url);
    };
    if !is_downloader_enabled(&config.global.disabled_downloaders, id.name()) {
        bail!("{} downloads are disabled", id.name());
    }

    let service =
        build_service(id, &config, args.password.as_deref()).map_err(anyhow::Error::msg)?;

    let staging = Staging::new(&args)?;
    let options = DownloadOptions {
        output_dir: staging.path.clone(),
        flat: false,
        resume: !args.no_resume && config.global.download.resume,
        on_conflict: config.global.download.on_conflict,
    };

    let downloaded = download_service(
        id.def().display_name.to_string(),
        &args.url,
        &service,
        &options,
        concurrency,
        false,
    )
    .await;
    output::print_download(std::slice::from_ref(&downloaded.batch));

    let download_failed = downloaded
        .batch
        .rows
        .iter()
        .filter(|row| row.outcome == DownloadOutcome::Failed)
        .count();
    let files = downloaded
        .files
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if files.is_empty() {
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

    println!();
    let upload_results = upload_to_uploaders(&files, &config, &upload_args).await?;
    output::print_upload(&upload_results);

    let upload_failed = upload_results
        .iter()
        .flat_map(|result| &result.rows)
        .filter(|row| row.status != "ok")
        .count();
    let failed = download_failed + upload_failed;
    if failed > 0 {
        bail!("{} mirror operation(s) failed", failed);
    }
    Ok(())
}

struct Staging {
    path: PathBuf,
    _temp: Option<TempDir>,
}

impl Staging {
    fn new(args: &MirrorArgs) -> Result<Self> {
        if let Some(path) = &args.staging {
            std::fs::create_dir_all(path)?;
            return Ok(Self {
                path: path.clone(),
                _temp: None,
            });
        }

        let temp = TempDir::new()?;
        if args.keep {
            let path = temp.keep();
            println!("staging: {}", path.display());
            Ok(Self { path, _temp: None })
        } else {
            let path = temp.path().to_path_buf();
            Ok(Self {
                path,
                _temp: Some(temp),
            })
        }
    }
}
