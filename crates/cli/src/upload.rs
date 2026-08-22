use std::sync::Arc;

use upio::registry::{self, UploaderId};
use upio_config::Config;

use crate::cli::UploadArgs;
use crate::output::{Row, UploaderResult};

/// Upload files to every requested uploader concurrently, then report per
/// uploader. Repeated `-u` names collapse to one run; disabled services are
/// reported and never constructed.
pub async fn upload_to_uploaders(
    files: &[String],
    config: &Config,
    args: &UploadArgs,
) -> anyhow::Result<Vec<UploaderResult>> {
    let files = Arc::new(files.to_vec());
    let config = Arc::new(config.clone());
    let folder_id = Arc::new(args.folder_id.clone());
    let folder_name = Arc::new(args.folder_name.clone());
    let batch_size = args.batch_size;

    // Preserve input order while collapsing repeated `-u` names.
    let mut requested: Vec<String> = Vec::new();
    for name in &args.uploaders {
        if !requested.contains(name) {
            requested.push(name.clone());
        }
    }

    let mut handles = Vec::new();
    let mut results = Vec::new();
    for name in &requested {
        if !upio_config::is_uploader_enabled(&config.global.disabled_uploaders, name) {
            results.push(UploaderResult {
                name: name.clone(),
                rows: vec![Row::message("disabled uploader")],
            });
            continue;
        }

        let files = files.clone();
        let config = config.clone();
        let folder_id = folder_id.clone();
        let folder_name = folder_name.clone();
        let name = name.clone();

        handles.push(tokio::spawn(async move {
            let rows =
                upload_one(&name, &files, &config, &folder_id, &folder_name, batch_size).await;
            (name, rows)
        }));
    }

    if handles.is_empty() {
        anyhow::bail!("no enabled uploaders selected");
    }

    for handle in handles {
        let (name, rows) = handle.await?;
        results.push(UploaderResult { name, rows });
    }
    Ok(results)
}

async fn upload_one(
    name: &str,
    files: &[String],
    config: &Config,
    folder_id: &Option<String>,
    folder_name: &Option<String>,
    batch_size: Option<usize>,
) -> Vec<Row> {
    let Some(id) = UploaderId::from_name(name) else {
        return vec![Row::message(format!("unknown uploader '{}'", name))];
    };

    let mut endpoint = config.get_uploader_config(id.name());
    if let Some(fid) = folder_id {
        endpoint.folder_id = Some(fid.clone());
    }

    let uploader = match registry::build_uploader(id, &endpoint) {
        Ok(uploader) => uploader,
        Err(e) => return vec![Row::message(e.to_string())],
    };

    if let Some(fname) = folder_name {
        match uploader.get_or_create_folder(fname, &endpoint).await {
            Ok(Some(fid)) => endpoint.folder_id = Some(fid),
            Ok(None) => {
                return vec![Row::message(format!(
                    "folder '{}' not found for '{}'",
                    fname, name
                ))]
            }
            Err(e) => return vec![Row::message(format!("folder lookup: {}", e))],
        }
    }

    upio::pipeline::upload_files(files, uploader.as_ref(), &endpoint, batch_size)
        .await
        .into_iter()
        .map(Row::from_result)
        .collect()
}
