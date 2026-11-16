use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use upio::http::UploadProgress;
use upio::pipeline;
use upio::registry::{self, UploaderId};
use upio_config::Config;

use crate::cli::UploadArgs;
use crate::live::{Handle, Live};
use crate::output::{self, Outcome, Phase, UploadView, UploaderResult};

/// Upload files to every requested uploader concurrently, then report per
/// uploader. Repeated `-u` names collapse to one run; disabled services are
/// reported and never constructed.
pub async fn upload_to_uploaders(
    files: &[String],
    config: &Config,
    args: &UploadArgs,
) -> anyhow::Result<Vec<UploaderResult>> {
    // Preserve input order while collapsing repeated `-u` names.
    let mut requested: Vec<String> = Vec::new();
    for name in &args.uploaders {
        if !requested.contains(name) {
            requested.push(name.clone());
        }
    }

    let mut uploaders = Vec::with_capacity(requested.len());
    let mut runnable = Vec::new();
    for name in &requested {
        if upio_config::is_uploader_enabled(&config.global.disabled_uploaders, name) {
            runnable.push((uploaders.len(), name.clone()));
            uploaders.push(UploaderResult::running(name.clone(), files));
        } else {
            uploaders.push(UploaderResult::notice(name.clone(), "disabled uploader"));
        }
    }

    if runnable.is_empty() {
        anyhow::bail!("no enabled uploaders selected");
    }

    let live = Live::start(UploadView { uploaders }, |view, screen| {
        output::upload_lines(&view.uploaders, screen, true)
    });

    let config = Arc::new(config.clone());
    let folder_id = Arc::new(args.folder_id.clone());
    let folder_name = Arc::new(args.folder_name.clone());
    let batch_size = args.batch_size;

    let mut handles = Vec::new();
    for (index, name) in runnable {
        let handle = live.handle();
        let files = files.to_vec();
        let config = Arc::clone(&config);
        let folder_id = Arc::clone(&folder_id);
        let folder_name = Arc::clone(&folder_name);
        handles.push(tokio::spawn(async move {
            upload_one(
                handle,
                index,
                &name,
                &files,
                &config,
                &folder_id,
                &folder_name,
                batch_size,
            )
            .await;
        }));
    }

    for handle in handles {
        handle.await?;
    }

    let mut view = live.view();
    for uploader in &mut view.uploaders {
        if uploader.phase == Phase::Running {
            uploader.finish();
        }
    }
    live.finish();
    Ok(view.uploaders)
}

#[allow(clippy::too_many_arguments)]
async fn upload_one(
    handle: Handle<UploadView>,
    index: usize,
    name: &str,
    files: &[String],
    config: &Config,
    folder_id: &Option<String>,
    folder_name: &Option<String>,
    batch_size: Option<usize>,
) {
    let Some(id) = UploaderId::from_name(name) else {
        replace_with_notice(&handle, index, format!("unknown uploader '{name}'"));
        return;
    };

    let mut endpoint = config.get_uploader_config(id.name());
    if let Some(fid) = folder_id {
        endpoint.folder_id = Some(fid.clone());
    }

    let uploader = match registry::build_uploader(id, &endpoint) {
        Ok(uploader) => uploader,
        Err(error) => {
            replace_with_notice(&handle, index, error.to_string());
            return;
        }
    };

    if let Some(fname) = folder_name {
        match uploader.get_or_create_folder(fname, &endpoint).await {
            Ok(Some(fid)) => endpoint.folder_id = Some(fid),
            Ok(None) => {
                replace_with_notice(
                    &handle,
                    index,
                    format!("folder '{fname}' not found for '{name}'"),
                );
                return;
            }
            Err(error) => {
                replace_with_notice(&handle, index, format!("folder lookup: {error}"));
                return;
            }
        }
    }

    let concurrency = batch_size.unwrap_or(1).max(1);
    let jobs = files
        .iter()
        .enumerate()
        .map(|(file_index, file)| {
            let handle = Handle::clone(&handle);
            let file = file.clone();
            let uploader = uploader.as_ref();
            let endpoint = &endpoint;
            async move {
                let started = Instant::now();
                let bytes = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
                handle.update(|view| {
                    let item = &mut view.uploaders[index].items[file_index];
                    item.outcome = Outcome::Running;
                    item.started = Some(started);
                    item.bytes = bytes;
                    item.total = 0;
                    item.transferred = 0;
                });

                let progress = progress_handle(&handle, index, file_index);
                let result =
                    pipeline::upload_file_with_progress(&file, uploader, endpoint, Some(progress))
                        .await;

                handle.update(|view| {
                    let item = &mut view.uploaders[index].items[file_index];
                    if let Some(error) = result.error {
                        item.fail(error);
                    } else {
                        item.done(bytes);
                        item.detail = result.urls.join(", ");
                    }
                });
            }
        })
        .collect::<Vec<_>>();

    futures::stream::iter(jobs)
        .buffered(concurrency)
        .count()
        .await;

    handle.update(|view| view.uploaders[index].finish());
}

fn progress_handle(handle: &Handle<UploadView>, index: usize, file_index: usize) -> UploadProgress {
    let handle = Handle::clone(handle);
    let last = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
    UploadProgress::new(move |transferred, total| {
        let now = Instant::now();
        {
            let mut last = last.lock().unwrap_or_else(|poison| poison.into_inner());
            if now.duration_since(*last) < Duration::from_millis(80) {
                return;
            }
            *last = now;
        }
        handle.update(|view| {
            let item = &mut view.uploaders[index].items[file_index];
            if item.outcome == Outcome::Running {
                item.transferred = transferred;
                item.total = total;
            }
        });
    })
}

fn replace_with_notice(handle: &Handle<UploadView>, index: usize, message: String) {
    handle.update(|view| {
        let mut result = UploaderResult::notice(view.uploaders[index].name.clone(), message);
        result.started = view.uploaders[index].started;
        view.uploaders[index] = result;
    });
}
