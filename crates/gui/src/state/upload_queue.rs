use std::path::PathBuf;
use std::time::Instant;

use dioxus::prelude::*;
use futures::StreamExt;
use upio::http::UploadProgress;
use upio::registry::{self, UploaderId};

use crate::components::ui::toast::{use_toast, ToastOptions, Toasts};
use crate::state::config_store::{use_config, ConfigStore};

/// The lifecycle of one queued file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStatus {
    /// Waiting for the user to confirm the upload.
    Pending,
    /// Being uploaded right now.
    Working,
    Done,
    Failed,
}

/// The lifecycle of one file × service attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeState {
    Waiting,
    Uploading,
    Done,
    Failed,
    /// The cooperative stop fired before this file/service started.
    Cancelled,
}

/// The result of uploading one file to one service.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceOutcome {
    pub service: UploaderId,
    pub state: OutcomeState,
    /// Cumulative `(uploaded, total)` bytes while [`OutcomeState::Uploading`].
    pub progress: Option<(u64, u64)>,
    pub urls: Vec<String>,
    pub error: Option<String>,
}

impl ServiceOutcome {
    fn pending(service: UploaderId) -> Self {
        Self {
            service,
            state: OutcomeState::Waiting,
            progress: None,
            urls: vec![],
            error: None,
        }
    }
}

/// One file in the upload queue.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub id: u64,
    pub file: PathBuf,
    pub size: u64,

    pub status: ItemStatus,
    pub outcomes: Vec<ServiceOutcome>,
}

impl QueueItem {
    /// Aggregate `(uploaded, total)` bytes across services, when known.
    pub fn progress(&self) -> Option<(u64, u64)> {
        let mut uploaded = 0u64;
        let mut total = 0u64;
        let mut any = false;
        for outcome in &self.outcomes {
            if let Some((up, tot)) = outcome.progress {
                uploaded += up;
                total += tot;
                any = true;
            }
        }
        any.then_some((uploaded, total))
    }
}

/// Access the upload queue. Must be called inside
/// [`UploadQueueProvider`](struct@UploadQueueProvider).
#[derive(Clone, Copy)]
pub struct UploadQueue {
    pub items: Signal<Vec<QueueItem>>,
    pub services: Signal<Vec<UploaderId>>,
    pub folder_name: Signal<String>,
    pub batch_size: Signal<usize>,
    pub running: Signal<bool>,
    /// Cooperative stop: when set, no further file/service starts after the
    /// in-flight upload finishes. Never aborts HTTP work.
    pub stop_requested: Signal<bool>,
    next_id: Signal<u64>,
    store: ConfigStore,
    toast: Toasts,
}

/// Get a handle to the upload queue.
pub fn use_queue() -> UploadQueue {
    use_context::<UploadQueue>()
}

impl UploadQueue {
    /// Add files as pending; nothing uploads until [`Self::confirm`].
    pub fn enqueue(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if self.services.read().is_empty() {
            self.toast.warning(
                "No services selected".to_string(),
                ToastOptions::new().description("Pick at least one service before uploading."),
            );
            return;
        }
        if *self.running.peek() {
            self.toast.warning(
                "Busy".to_string(),
                ToastOptions::new().description("Wait for the current uploads to finish."),
            );
            return;
        }

        let services = self.services.peek().clone();

        let mut next_id = self.next_id.write();
        let mut items = self.items.write();
        for path in paths {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            items.push(QueueItem {
                id: *next_id,
                file: path,
                size,
                status: ItemStatus::Pending,
                outcomes: services
                    .iter()
                    .map(|s| ServiceOutcome::pending(*s))
                    .collect(),
            });
            *next_id += 1;
        }
    }

    /// Apply a new service selection to the queued work.
    ///
    /// Returns `false` and leaves state untouched while uploads are running.
    /// Otherwise updates `services` (dropping disabled ids) and rewrites the
    /// outcome slots of every still-`Pending` item so confirmation uploads
    /// exactly what the UI shows. An empty selection is allowed so the UI can
    /// explain why upload is unavailable.
    pub fn set_services(&mut self, ids: Vec<UploaderId>) -> bool {
        if *self.running.peek() {
            return false;
        }
        let disabled = self.store.disabled_uploaders();
        let enabled_ids: Vec<UploaderId> = ids
            .into_iter()
            .filter(|id| !disabled.contains(id.name()))
            .collect();

        let mut items = self.items.write();
        for item in items.iter_mut() {
            if item.status != ItemStatus::Pending {
                continue;
            }
            item.outcomes = enabled_ids
                .iter()
                .map(|service| ServiceOutcome::pending(*service))
                .collect();
        }
        drop(items);
        self.services.set(enabled_ids);
        true
    }

    /// Remove a single item that has not started yet.
    pub fn discard(&mut self, id: u64) {
        self.items
            .write()
            .retain(|item| !(item.id == id && item.status == ItemStatus::Pending));
    }

    /// Promote every pending item to active and start the runner.
    pub fn confirm(&mut self) {
        let mut items = self.items.write();
        for item in items.iter_mut() {
            if item.status == ItemStatus::Pending {
                item.status = ItemStatus::Working;
            }
        }
        drop(items);
        self.start();
    }

    /// Remove files whose upload finished (successfully or not).
    pub fn clear_finished(&mut self) {
        self.items
            .write()
            .retain(|item| matches!(item.status, ItemStatus::Pending | ItemStatus::Working));
    }

    /// Empty the queue; returns false while uploads are running.
    pub fn clear_all(&mut self) -> bool {
        if *self.running.peek() {
            return false;
        }
        self.items.clear();
        true
    }

    /// Request a cooperative stop: the in-flight upload finishes, then no
    /// further file/service starts. Remaining work is marked cancelled.
    pub fn request_stop(&mut self) {
        if *self.running.peek() {
            self.stop_requested.set(true);
        }
    }

    fn start(&mut self) {
        if *self.running.peek() {
            return;
        }
        self.stop_requested.set(false);
        self.running.set(true);

        let mut items = self.items;
        let mut running = self.running;
        let stop_requested = self.stop_requested;
        let folder_name = self.folder_name;
        let batch_size = (*self.batch_size.peek()).max(1);
        let store = self.store;
        spawn(async move {
            loop {
                // Snapshot jobs so edits to the list mid-run are safe.
                let jobs: Vec<QueueItem> = items
                    .peek()
                    .iter()
                    .filter(|item| item.status == ItemStatus::Working)
                    .cloned()
                    .collect();
                if jobs.is_empty() {
                    break;
                }

                // Stop is cooperative: files already in flight finish, but no
                // new file starts once the flag is observed.
                if *stop_requested.peek() {
                    break;
                }

                let results = futures::stream::iter(jobs)
                    .map(|item| {
                        let mut items = items;
                        async move {
                            let outcomes =
                                run_file(&item, &mut items, &folder_name, &store, &stop_requested)
                                    .await;
                            (item.id, outcomes)
                        }
                    })
                    .buffer_unordered(batch_size)
                    .collect::<Vec<(u64, Vec<ServiceOutcome>)>>()
                    .await;

                let mut list = items.write();
                for (id, outcomes) in results {
                    if let Some(item) = list.iter_mut().find(|item| item.id == id) {
                        let failed = outcomes.iter().any(|o| o.error.is_some());
                        item.status = if failed {
                            ItemStatus::Failed
                        } else {
                            ItemStatus::Done
                        };
                        // Preserve live progress written during the run; only
                        // replace slots that are still waiting.
                        for outcome in &mut item.outcomes {
                            if outcome.state == OutcomeState::Waiting {
                                if let Some(final_state) =
                                    outcomes.iter().find(|o| o.service == outcome.service)
                                {
                                    outcome.state = final_state.state;
                                    outcome.urls = final_state.urls.clone();
                                    outcome.error = final_state.error.clone();
                                    outcome.progress = final_state.progress;
                                }
                            } else if let Some(final_state) =
                                outcomes.iter().find(|o| o.service == outcome.service)
                            {
                                outcome.urls = final_state.urls.clone();
                                outcome.error = final_state.error.clone();
                            }
                        }
                    }
                }
            }
            // Any file still marked Working after the loop was skipped by a
            // cooperative stop: publish its cancelled state.
            if *stop_requested.peek() {
                let mut list = items.write();
                for item in list.iter_mut() {
                    if item.status == ItemStatus::Working {
                        for outcome in &mut item.outcomes {
                            if matches!(
                                outcome.state,
                                OutcomeState::Waiting | OutcomeState::Uploading
                            ) {
                                outcome.state = OutcomeState::Cancelled;
                                outcome.progress = None;
                                if outcome.error.is_none() {
                                    outcome.error = Some("cancelled".to_string());
                                }
                            }
                        }
                        item.status = ItemStatus::Failed;
                    }
                }
            }

            running.set(false);
        });
    }
}

/// Run one queued file against every selected service, streaming live
/// per-service state/progress updates back into `items`.
async fn run_file(
    item: &QueueItem,
    items: &mut Signal<Vec<QueueItem>>,
    folder_name: &Signal<String>,
    store: &ConfigStore,
    stop_requested: &Signal<bool>,
) -> Vec<ServiceOutcome> {
    let path = item.file.to_string_lossy().into_owned();
    let folder = folder_name.read().trim().to_string();
    let folder = (!folder.is_empty()).then_some(folder);

    // Publish a fresh Waiting slot per service so the UI can render them.
    {
        let mut list = items.write();
        if let Some(entry) = list.iter_mut().find(|entry| entry.id == item.id) {
            entry.outcomes = item.outcomes.clone();
        }
    }

    let mut outcomes: Vec<ServiceOutcome> = item
        .outcomes
        .iter()
        .map(|outcome| ServiceOutcome::pending(outcome.service))
        .collect();

    for slot in outcomes.iter_mut() {
        let service = slot.service;

        // Cooperative stop: never start the next service once the user
        // requested a stop; the in-flight one (if any) has already finished.
        if *stop_requested.peek() {
            slot.state = OutcomeState::Cancelled;
            slot.error = Some("cancelled".to_string());
            continue;
        }

        slot.state = OutcomeState::Uploading;
        // Live slot update helper.
        let publish = |items: &mut Signal<Vec<QueueItem>>, updated: &ServiceOutcome| {
            let mut list = items.write();
            if let Some(entry) = list.iter_mut().find(|entry| entry.id == item.id) {
                if let Some(target) = entry
                    .outcomes
                    .iter_mut()
                    .find(|target| target.service == service)
                {
                    *target = updated.clone();
                }
            }
        };
        publish(items, slot);
        if slot.state == OutcomeState::Cancelled {
            continue;
        }

        let mut endpoint = store.endpoint(service);

        let uploader = match registry::build_uploader(service, &endpoint) {
            Ok(uploader) => uploader,
            Err(e) => {
                slot.state = OutcomeState::Failed;
                slot.error = Some(e.to_string());
                publish(items, slot);
                continue;
            }
        };

        // Resolve the album/folder by name when the service supports it,
        // using the same uploader instance that performs the upload.
        if let Some(name) = &folder {
            match uploader.get_or_create_folder(name, &endpoint).await {
                Ok(Some(folder_id)) => endpoint.folder_id = Some(folder_id),
                Ok(None) => {}
                Err(e) => {
                    slot.state = OutcomeState::Failed;
                    slot.error = Some(format!("folder '{}': {}", name, e));
                    publish(items, slot);
                    continue;
                }
            }
        }

        if let Err(e) = uploader.init().await {
            slot.state = OutcomeState::Failed;
            slot.error = Some(e.to_string());
            publish(items, slot);
            continue;
        }

        // Progress is reported through a channel; a small forwarder task is
        // what actually touches the (non-Sync) items signal, throttled.
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64)>();
        let progress = UploadProgress::new(move |uploaded, total| {
            let _ = progress_tx.send((uploaded, total));
        });

        let mut forward_items = *items;
        let forward_target = item.id;
        let service_id = slot.service;

        // The forwarder keeps using Dioxus's task runner because it touches
        // the (non-Send) items signal; a oneshot handshake lets us observe
        // its completion so it never outlives the item.
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let _forwarder = spawn(async move {
            let mut last_emit = Option::<Instant>::None;
            while let Some((uploaded, total)) = progress_rx.recv().await {
                let now = Instant::now();
                if let Some(previous) = last_emit {
                    if now.duration_since(previous).as_millis() < 100 {
                        continue;
                    }
                }
                last_emit = Some(now);

                let mut list = forward_items.write();
                if let Some(entry) = list.iter_mut().find(|entry| entry.id == forward_target) {
                    if let Some(target) = entry
                        .outcomes
                        .iter_mut()
                        .find(|target| target.service == service_id)
                    {
                        target.progress = Some((uploaded, total));
                    }
                }
            }
            // Loop ended: the channel closed and the forwarder is done.
            let _ = done_tx.send(());
        });

        // The upload owns the sender clone inside `progress`; when the
        // pipeline call returns, the channel closes and the forwarder drains
        // to completion before the final publish below.
        let result =
            upio::pipeline::upload_file_with_progress(&path, &*uploader, &endpoint, Some(progress))
                .await;
        // Wait for the forwarder's completion handshake: no task survives
        // past the item, and the final publish below cannot be overwritten.
        let _ = done_rx.await;

        slot.progress = None;
        slot.urls = result.urls;
        slot.error = result.error;
        slot.state = if slot.error.is_some() {
            OutcomeState::Failed
        } else {
            OutcomeState::Done
        };
        publish(items, slot);
    }

    outcomes
}

/// Provides the upload queue to the subtree. Must be nested inside
/// [`ConfigProvider`](crate::state::config_store::ConfigProvider).
#[component]
pub fn UploadQueueProvider(children: Element) -> Element {
    let toast = use_toast();
    let store = use_config();
    let mut gui = crate::state::gui_state::use_gui_state();

    // Seed from the remembered upload settings, falling back to every
    // service the config leaves enabled. Remembered names for since-disabled
    // services are filtered out.
    let default_services: Vec<UploaderId> = {
        let disabled = store.disabled_uploaders();
        let remembered: Vec<UploaderId> = gui
            .state
            .peek()
            .upload
            .services
            .iter()
            .filter_map(|name| UploaderId::from_name(name))
            .filter(|id| !disabled.contains(id.name()))
            .collect();
        if remembered.is_empty() {
            UploaderId::ALL
                .iter()
                .copied()
                .filter(|id| !disabled.contains(id.name()))
                .collect()
        } else {
            remembered
        }
    };
    let items = Signal::new(Vec::new());
    let services = Signal::new(default_services);
    let folder_name = Signal::new(String::new());
    let batch_size = Signal::new(gui.state.peek().upload.batch_size.max(1));

    // Remember changes to both settings.
    use_effect(move || {
        let names: Vec<String> = services
            .read()
            .iter()
            .map(|id| id.name().to_string())
            .collect();
        gui.state.write().upload.services = names;
    });
    use_effect(move || {
        // Read (not peek) so this effect re-runs — and persists — whenever
        // the user changes concurrency.
        let size = *batch_size.read();
        gui.state.write().upload.batch_size = size;
    });

    use_context_provider(|| UploadQueue {
        items,
        services,
        folder_name,
        batch_size,
        running: Signal::new(false),
        stop_requested: Signal::new(false),
        next_id: Signal::new(0),
        store,
        toast,
    });

    rsx! {
        {children}
    }
}
