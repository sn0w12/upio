use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures::StreamExt;

use rune::AppHandle;
use upio::http::UploadProgress;
use upio::pipeline;
use upio::registry::{self, UploaderId};
use upio_config::Config;

use crate::app::App;

pub enum Command {
    Upload {
        jobs: Vec<(u64, PathBuf)>,
        services: Vec<UploaderId>,
        folder: Option<String>,
        concurrency: usize,
        config: Box<Config>,
    },
    Stop,
    ProbeTools,
}

static COMMANDS: OnceLock<tokio::sync::mpsc::UnboundedSender<Command>> = OnceLock::new();

pub fn send(command: Command) -> bool {
    COMMANDS
        .get()
        .is_some_and(|sender| sender.send(command).is_ok())
}

pub fn start(handle: AppHandle<App>) {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    if COMMANDS.set(sender).is_err() {
        return;
    }
    let _ = send(Command::ProbeTools);

    let worker_handle = handle.clone();
    let result = std::thread::Builder::new()
        .name("upio-gui-worker".to_string())
        .spawn(move || {
            let handle = worker_handle;
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("upio-gui-worker")
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    notify(
                        &handle,
                        format!("Could not start the upload worker: {error}"),
                    );
                    return;
                }
            };

            runtime.block_on(async move {
                let stop = Arc::new(AtomicBool::new(false));
                let active = Arc::new(AtomicBool::new(false));
                while let Some(command) = receiver.recv().await {
                    match command {
                        Command::Upload {
                            jobs,
                            services,
                            folder,
                            concurrency,
                            config,
                        } => {
                            if active.swap(true, Ordering::SeqCst) {
                                notify(&handle, "An upload is already running.".to_string());
                                continue;
                            }
                            stop.store(false, Ordering::SeqCst);
                            let handle = handle.clone();
                            let stop = stop.clone();
                            let active = active.clone();
                            tokio::spawn(async move {
                                run_upload(
                                    handle.clone(),
                                    jobs,
                                    services,
                                    folder,
                                    concurrency,
                                    Arc::new(*config),
                                    stop,
                                )
                                .await;
                                active.store(false, Ordering::SeqCst);
                                handle.update(|app, _| app.batch_done());
                            });
                        }
                        Command::Stop => stop.store(true, Ordering::SeqCst),
                        Command::ProbeTools => {
                            let handle = handle.clone();
                            tokio::spawn(async move {
                                let tools = tokio::task::spawn_blocking(
                                    upio::preprocess::tools::available_tools,
                                )
                                .await
                                .unwrap_or_default();
                                handle.update(move |app, _| app.set_tools(tools));
                            });
                        }
                    }
                }
            });
        });

    if let Err(error) = result {
        notify(
            &handle,
            format!("Could not start the upload worker thread: {error}"),
        );
    }
}

fn notify(handle: &AppHandle<App>, message: String) {
    handle.update(move |app, _| app.set_notice(message));
}

async fn run_upload(
    handle: AppHandle<App>,
    jobs: Vec<(u64, PathBuf)>,
    services: Vec<UploaderId>,
    folder: Option<String>,
    concurrency: usize,
    config: Arc<Config>,
    stop: Arc<AtomicBool>,
) {
    let concurrency = concurrency.max(1);

    futures::stream::iter(jobs.into_iter().map(|(item, path)| {
        let handle = handle.clone();
        let services = services.clone();
        let folder = folder.clone();
        let config = config.clone();
        let stop = stop.clone();
        async move {
            for service in services {
                if stop.load(Ordering::SeqCst) {
                    let handle = handle.clone();
                    handle.update(move |app, _| {
                        app.service_done(
                            item,
                            service,
                            Vec::new(),
                            Some("cancelled".to_string()),
                            true,
                        );
                    });
                    continue;
                }

                let handle_for_start = handle.clone();
                handle_for_start.update(move |app, _| app.service_started(item, service));

                let mut endpoint = config.get_uploader_config(service.name());
                let uploader = match registry::build_uploader(service, &endpoint) {
                    Ok(uploader) => uploader,
                    Err(error) => {
                        let message = error.to_string();
                        let handle = handle.clone();
                        handle.update(move |app, _| {
                            app.service_done(item, service, Vec::new(), Some(message), false)
                        });
                        continue;
                    }
                };

                if let Some(name) = folder.as_deref() {
                    match uploader.get_or_create_folder(name, &endpoint).await {
                        Ok(Some(folder_id)) => endpoint.folder_id = Some(folder_id),
                        Ok(None) => {}
                        Err(error) => {
                            let message = format!("folder '{name}': {error}");
                            let handle = handle.clone();
                            handle.update(move |app, _| {
                                app.service_done(item, service, Vec::new(), Some(message), false)
                            });
                            continue;
                        }
                    }
                }

                if stop.load(Ordering::SeqCst) {
                    let handle = handle.clone();
                    handle.update(move |app, _| {
                        app.service_done(
                            item,
                            service,
                            Vec::new(),
                            Some("cancelled".to_string()),
                            true,
                        );
                    });
                    continue;
                }

                let progress_handle = handle.clone();
                let last = Arc::new(Mutex::new(None::<Instant>));
                let progress = UploadProgress::new(move |uploaded, total| {
                    let now = Instant::now();
                    let should_send = {
                        let mut last = last.lock().unwrap_or_else(|poison| poison.into_inner());
                        match *last {
                            Some(previous)
                                if now.duration_since(previous) < Duration::from_millis(100) =>
                            {
                                false
                            }
                            _ => {
                                *last = Some(now);
                                true
                            }
                        }
                    };
                    if should_send {
                        let handle = progress_handle.clone();
                        handle.update(move |app, _| {
                            app.service_progress(item, service, uploaded, total)
                        });
                    }
                });

                let path_text = path.to_string_lossy().into_owned();
                let result = pipeline::upload_file_with_progress(
                    &path_text,
                    uploader.as_ref(),
                    &endpoint,
                    Some(progress),
                )
                .await;

                let handle = handle.clone();
                handle.update(move |app, _| {
                    app.service_done(item, service, result.urls, result.error, false)
                });
            }

            let handle = handle.clone();
            handle.update(move |app, _| app.file_done(item));
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await;
}
