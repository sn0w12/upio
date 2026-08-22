use std::path::Path;

use anyhow::{bail, Result};

use crate::cli::UploadArgs;
use crate::input::collect_files;
use crate::output;
use crate::upload::upload_to_uploaders;
use upio_config::Settings;

/// Run the `upload` command: collect files, load settings, and upload.
pub async fn run(args: UploadArgs, config_path: Option<&Path>) -> Result<()> {
    let files = collect_files(&args.paths, &args.glob)?;
    if files.is_empty() {
        bail!("No files to upload");
    }

    let settings = Settings::load_from_or_default(config_path)?;
    let config = settings.extract()?;

    let results = upload_to_uploaders(&files, &config, &args).await?;
    output::print_upload(&results);
    Ok(())
}
