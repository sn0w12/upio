use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rune::{TextEdit, Tx};
use serde::{Deserialize, Serialize};
use upio::preprocess::tools::Tool;
use upio::preprocess::{PreprocessConfig, PreprocessRule, PreprocessStrategy};
use upio::registry::UploaderId;
use upio_config::{Config, ConfigKey};

use crate::theme::{kit, ThemeMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Upload,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Services,
    Tools,
    Advanced,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 4] = [
        Self::Appearance,
        Self::Services,
        Self::Tools,
        Self::Advanced,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Services => "Services",
            Self::Tools => "Tools",
            Self::Advanced => "Advanced",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Pending,
    Working,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeState {
    Waiting,
    Uploading,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct ServiceOutcome {
    pub service: UploaderId,
    pub state: OutcomeState,
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
            urls: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub id: u64,
    pub path: PathBuf,
    pub size: u64,
    pub status: ItemStatus,
    pub outcomes: Vec<ServiceOutcome>,
}

impl QueueItem {
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn progress(&self) -> Option<(u64, u64)> {
        let mut uploaded = 0;
        let mut total = 0;
        let mut any = false;
        for outcome in &self.outcomes {
            if let Some((u, t)) = outcome.progress {
                uploaded += u;
                total += t;
                any = true;
            }
        }
        any.then_some((uploaded, total))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GuiPrefs {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

fn default_concurrency() -> usize {
    4
}

impl GuiPrefs {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("upio")
            .join("gui.toml")
    }

    fn load() -> Result<Self, String> {
        let path = Self::path();
        let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        toml::from_str(&text).map_err(|error| error.to_string())
    }

    fn save(&self) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
        upio_config::write_atomic(&Self::path(), text.as_bytes()).map_err(|error| error.to_string())
    }
}

pub struct App {
    pub page: Page,
    pub settings_section: SettingsSection,
    pub theme: ThemeMode,
    pub open: Option<String>,
    pub show_raw: bool,
    pub revealed: HashSet<ConfigKey>,

    pub config: Config,
    pub baseline: Config,
    pub edits: HashMap<ConfigKey, TextEdit>,
    pub raw: TextEdit,
    pub dirty: bool,
    pub save_error: Option<String>,
    pub load_error: Option<String>,
    pub settings_service: UploaderId,
    pub settings_global: bool,

    pub queue: Vec<QueueItem>,
    next_id: u64,
    pub services: Vec<UploaderId>,
    pub concurrency: usize,
    pub folder: TextEdit,
    pub running: bool,
    pub stopping: bool,

    pub tools: Vec<(Tool, bool)>,
    pub drop_hover: bool,
    pub notice: Option<String>,
    notice_until: Option<Instant>,
}

impl App {
    pub fn new() -> Self {
        let (prefs, gui_error) = match GuiPrefs::load() {
            Ok(prefs) => (prefs, None),
            Err(_) if !GuiPrefs::path().exists() => (GuiPrefs::default(), None),
            Err(error) => (GuiPrefs::default(), Some(error)),
        };
        let (config, load_error) = match Config::load() {
            Ok(config) => (config, None),
            Err(error) => (Config::default(), Some(error.to_string())),
        };
        if !Config::path().is_file() {
            let _ = config.save();
        }

        let active = active_services(&config);
        let mut services: Vec<UploaderId> = prefs
            .services
            .iter()
            .filter_map(|name| UploaderId::from_name(name))
            .filter(|id| active.contains(id))
            .collect();
        if services.is_empty() {
            services = active.clone();
        }
        let settings_service = enabled_services(&config)
            .first()
            .copied()
            .unwrap_or(UploaderId::ALL[0]);

        let edits = ConfigKey::ALL
            .iter()
            .map(|key| (*key, TextEdit::new(config.get_value(key))))
            .collect();
        let raw = TextEdit::new(config.to_toml().unwrap_or_default());

        let mut app = Self {
            page: Page::Upload,
            settings_section: SettingsSection::Appearance,
            theme: prefs.theme,
            open: None,
            show_raw: false,
            revealed: HashSet::new(),
            config,
            baseline: Config::default(),
            edits,
            raw,
            dirty: false,
            save_error: None,
            load_error,
            settings_service,
            settings_global: false,
            queue: Vec::new(),
            next_id: 1,
            services,
            concurrency: prefs.concurrency.clamp(1, 8),
            folder: TextEdit::new(""),
            running: false,
            stopping: false,
            tools: Vec::new(),
            drop_hover: false,
            notice: None,
            notice_until: None,
        };
        app.baseline = app.config.clone();
        if let Some(error) = gui_error {
            app.set_notice(format!("Could not read gui.toml: {error}"));
        }
        app
    }

    pub fn dark(&self, system_dark: bool) -> bool {
        self.theme.dark(system_dark)
    }

    pub fn kit(&self, system_dark: bool) -> rune_kit::Kit {
        kit(self.dark(system_dark))
    }

    pub fn set_page(&mut self, page: Page) {
        self.page = page;
        self.open = None;
    }

    pub fn toggle_open(&mut self, key: &str) {
        self.open = if self.open.as_deref() == Some(key) {
            None
        } else {
            Some(key.to_string())
        };
    }

    pub fn close_open(&mut self) {
        self.open = None;
    }

    pub fn is_open(&self, key: &str) -> bool {
        self.open.as_deref() == Some(key)
    }

    pub fn toggle_reveal(&mut self, key: ConfigKey) {
        if !self.revealed.remove(&key) {
            self.revealed.insert(key);
        }
    }

    pub fn active_services(&self) -> Vec<UploaderId> {
        active_services(&self.config)
    }

    pub fn service_enabled(&self, id: UploaderId) -> bool {
        !self
            .config
            .global
            .disabled_uploaders
            .iter()
            .any(|name| name == id.name())
    }

    pub fn service_ready(&self, id: UploaderId) -> bool {
        missing_credential(&self.config, id).is_none()
    }

    pub fn missing_credential(&self, id: UploaderId) -> Option<&'static str> {
        missing_credential(&self.config, id)
    }

    pub fn service_active(&self, id: UploaderId) -> bool {
        self.service_enabled(id) && self.service_ready(id)
    }

    pub fn downloader_enabled(&self, id: UploaderId) -> bool {
        upio_config::is_downloader_enabled(&self.config.global.disabled_downloaders, id.name())
    }

    pub fn set_downloader_enabled(&mut self, id: UploaderId, enabled: bool) {
        let mut disabled = self.config.global.disabled_downloaders.clone();
        let name = id.name().to_string();
        if enabled {
            disabled.retain(|entry| entry != &name);
        } else if !disabled.contains(&name) {
            disabled.push(name);
        }
        disabled.retain(|entry| {
            UploaderId::ALL
                .iter()
                .any(|known| known.name() == entry.as_str())
        });
        let ordered: Vec<String> = UploaderId::ALL
            .iter()
            .map(|known| known.name().to_string())
            .filter(|name| disabled.contains(name))
            .collect();
        self.config.global.disabled_downloaders = ordered;
        self.dirty = true;
    }

    pub fn selected(&self, id: UploaderId) -> bool {
        self.services.contains(&id)
    }

    pub fn toggle_service(&mut self, id: UploaderId) {
        if self.running {
            self.set_notice("Wait for the current uploads to finish before changing services.");
            return;
        }
        if !self.service_active(id) {
            return;
        }
        if let Some(index) = self.services.iter().position(|service| *service == id) {
            self.services.remove(index);
        } else {
            self.services.push(id);
        }
        self.sync_pending_outcomes();
        self.persist_prefs();
    }

    pub fn set_services(&mut self, ids: Vec<UploaderId>) {
        if self.running {
            self.set_notice("Wait for the current uploads to finish before changing services.");
            return;
        }
        self.services = ids
            .into_iter()
            .filter(|id| self.service_active(*id))
            .collect();
        self.sync_pending_outcomes();
        self.persist_prefs();
    }

    pub fn set_concurrency(&mut self, value: usize) {
        self.concurrency = value.clamp(1, 8);
        self.persist_prefs();
    }

    pub fn set_theme(&mut self, theme: ThemeMode) {
        self.theme = theme;
        self.persist_prefs();
    }

    pub fn persist_prefs(&mut self) {
        let prefs = GuiPrefs {
            theme: self.theme,
            services: self
                .services
                .iter()
                .map(|id| id.name().to_string())
                .collect(),
            concurrency: self.concurrency,
        };
        if let Err(error) = prefs.save() {
            self.set_notice(format!("Could not save gui.toml: {error}"));
        }
    }

    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        if self.running {
            self.set_notice("Wait for the current uploads to finish before adding files.");
            return;
        }
        if self.services.is_empty() {
            self.set_notice("Pick at least one service before adding files.");
            return;
        }
        let services = self.services.clone();
        let mut added = 0;
        for path in paths {
            let files: Vec<PathBuf> = if path.is_dir() {
                walkdir::WalkDir::new(&path)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_file())
                    .map(|entry| entry.into_path())
                    .collect()
            } else {
                vec![path]
            };
            for path in files {
                if !path.is_file() {
                    continue;
                }
                if self
                    .queue
                    .iter()
                    .any(|item| item.path == path && item.status == ItemStatus::Pending)
                {
                    continue;
                }
                let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                self.queue.push(QueueItem {
                    id: self.next_id,
                    path,
                    size,
                    status: ItemStatus::Pending,
                    outcomes: services
                        .iter()
                        .copied()
                        .map(ServiceOutcome::pending)
                        .collect(),
                });
                self.next_id += 1;
                added += 1;
            }
        }
        if added == 0 {
            self.set_notice("No new files were added.");
        } else {
            self.notice = None;
        }
    }

    pub fn discard(&mut self, id: u64) {
        self.queue
            .retain(|item| !(item.id == id && item.status == ItemStatus::Pending));
    }

    pub fn clear_finished(&mut self) {
        self.queue
            .retain(|item| matches!(item.status, ItemStatus::Pending | ItemStatus::Working));
    }

    pub fn confirm(&mut self) {
        if self.running {
            return;
        }
        if self.services.is_empty() {
            self.set_notice("Pick at least one service before uploading.");
            return;
        }
        let jobs: Vec<(u64, PathBuf)> = self
            .queue
            .iter_mut()
            .filter(|item| item.status == ItemStatus::Pending)
            .map(|item| {
                item.status = ItemStatus::Working;
                item.outcomes = self
                    .services
                    .iter()
                    .copied()
                    .map(ServiceOutcome::pending)
                    .collect();
                (item.id, item.path.clone())
            })
            .collect();
        if jobs.is_empty() {
            self.set_notice("There are no pending files to upload.");
            return;
        }

        let folder = self.folder.text.trim().to_string();
        let command = crate::worker::Command::Upload {
            jobs,
            services: self.services.clone(),
            folder: (!folder.is_empty()).then_some(folder),
            concurrency: self.concurrency,
            config: Box::new(self.config.clone()),
        };
        if crate::worker::send(command) {
            self.running = true;
            self.stopping = false;
            self.notice = None;
        } else {
            for item in &mut self.queue {
                if item.status == ItemStatus::Working {
                    item.status = ItemStatus::Failed;
                }
            }
            self.set_notice("The upload worker is not available.");
        }
    }

    pub fn request_stop(&mut self) {
        if self.running && !self.stopping {
            self.stopping = true;
            let _ = crate::worker::send(crate::worker::Command::Stop);
        }
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.notice_until = Some(Instant::now() + Duration::from_secs(6));
    }

    pub fn poll_notice(&mut self) -> bool {
        if self
            .notice_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.notice = None;
            self.notice_until = None;
            return true;
        }
        false
    }

    pub fn service_started(&mut self, item: u64, service: UploaderId) {
        if let Some(outcome) = outcome_mut(&mut self.queue, item, service) {
            outcome.state = OutcomeState::Uploading;
            outcome.error = None;
        }
    }

    pub fn service_progress(&mut self, item: u64, service: UploaderId, uploaded: u64, total: u64) {
        if let Some(outcome) = outcome_mut(&mut self.queue, item, service) {
            outcome.progress = Some((uploaded, total));
        }
    }

    pub fn service_done(
        &mut self,
        item: u64,
        service: UploaderId,
        urls: Vec<String>,
        error: Option<String>,
        cancelled: bool,
    ) {
        if let Some(outcome) = outcome_mut(&mut self.queue, item, service) {
            outcome.progress = None;
            outcome.urls = urls;
            outcome.error = error;
            outcome.state = if cancelled {
                OutcomeState::Cancelled
            } else if outcome.error.is_some() {
                OutcomeState::Failed
            } else {
                OutcomeState::Done
            };
        }
    }

    pub fn file_done(&mut self, item: u64) {
        if let Some(entry) = self.queue.iter_mut().find(|entry| entry.id == item) {
            entry.status = if entry.outcomes.iter().any(|outcome| {
                matches!(
                    outcome.state,
                    OutcomeState::Failed | OutcomeState::Cancelled
                )
            }) {
                ItemStatus::Failed
            } else {
                ItemStatus::Done
            };
        }
    }

    pub fn batch_done(&mut self) {
        self.running = false;
        self.stopping = false;
    }

    pub fn set_tools(&mut self, tools: Vec<(Tool, bool)>) {
        self.tools = tools;
    }

    pub fn set_enabled(&mut self, id: UploaderId, enabled: bool) {
        if enabled {
            if let Some(missing) = self.missing_credential(id) {
                self.set_notice(format!(
                    "Add the {missing} for {} before enabling it.",
                    id.def().display_name
                ));
                return;
            }
        }
        let mut disabled: Vec<String> = self.config.global.disabled_uploaders.clone();
        let name = id.name().to_string();
        if enabled {
            disabled.retain(|entry| entry != &name);
        } else if !disabled.contains(&name) {
            disabled.push(name);
        }
        disabled.retain(|entry| {
            UploaderId::ALL
                .iter()
                .any(|known| known.name() == entry.as_str())
        });
        let ordered: Vec<String> = UploaderId::ALL
            .iter()
            .map(|known| known.name().to_string())
            .filter(|name| disabled.contains(name))
            .collect();
        self.config.global.disabled_uploaders = ordered;
        self.dirty = true;
        self.sync_pending_outcomes();
        if !enabled {
            self.services.retain(|service| *service != id);
            self.persist_prefs();
        }
    }

    pub fn toggle_preprocess(&mut self, id: UploaderId, strategy: PreprocessStrategy, on: bool) {
        let Some(preprocess) = self.config.preprocess_mut(id.name()) else {
            return;
        };
        set_strategy(preprocess, strategy, on);
        self.dirty = true;
    }

    pub fn preprocess_enabled(&mut self, id: UploaderId, enabled: bool) {
        if let Some(preprocess) = self.config.preprocess_mut(id.name()) {
            preprocess.enabled = enabled;
        }
        self.dirty = true;
    }

    pub fn preprocess_has(&self, id: UploaderId, strategy: PreprocessStrategy) -> bool {
        self.config
            .preprocess_for(id.name())
            .rules
            .iter()
            .any(|rule| rule.strategy == strategy)
    }

    pub fn toggle_preprocess_strategy(&mut self, id: UploaderId, strategy: PreprocessStrategy) {
        let on = !self.preprocess_has(id, strategy);
        self.toggle_preprocess(id, strategy, on);
    }

    pub fn preprocess_is_overridden(&self, id: UploaderId) -> bool {
        self.config.preprocess_override(id.name()).is_some()
    }

    pub fn clear_preprocess_override(&mut self, id: UploaderId) {
        if self.config.clear_preprocess_override(id.name()) {
            self.dirty = true;
        }
    }

    pub fn set_preprocess_override(&mut self, id: UploaderId, overridden: bool) {
        if overridden {
            if self.config.preprocess_mut(id.name()).is_some() {
                self.dirty = true;
            }
        } else {
            self.clear_preprocess_override(id);
        }
    }

    pub fn set_global_preprocess_enabled(&mut self, enabled: bool) {
        self.config.preprocess.enabled = enabled;
        self.dirty = true;
    }

    pub fn global_preprocess_has(&self, strategy: PreprocessStrategy) -> bool {
        self.config
            .preprocess
            .rules
            .iter()
            .any(|rule| rule.strategy == strategy)
    }

    pub fn toggle_global_preprocess_strategy(&mut self, strategy: PreprocessStrategy) {
        let on = !self.global_preprocess_has(strategy);
        set_strategy(&mut self.config.preprocess, strategy, on);
        self.dirty = true;
    }

    pub fn commit_field(&mut self, key: ConfigKey) {
        let value = self
            .edits
            .get(&key)
            .map(|edit| edit.text.clone())
            .unwrap_or_default();
        if let Err(error) = self.config.set_value(&key, &value) {
            self.save_error = Some(error.to_string());
            return;
        }
        self.dirty = true;
        self.save_error = None;
        let active = self.active_services();
        self.services.retain(|id| active.contains(id));
        self.sync_pending_outcomes();
    }

    /// Set a config key from a control that is not a text field, keeping the
    /// text editors in sync.
    pub fn set_config_value(&mut self, key: ConfigKey, value: &str) {
        match self.config.set_value(&key, value) {
            Ok(()) => {
                if let Some(edit) = self.edits.get_mut(&key) {
                    edit.replace_text(self.config.get_value(&key));
                }
                self.dirty = true;
                self.save_error = None;
            }
            Err(error) => {
                self.save_error = Some(error.to_string());
            }
        }
    }

    pub fn save_config(&mut self) {
        if self.load_error.is_some() {
            self.save_error = Some("Fix or reload the config file before saving.".to_string());
            return;
        }
        match self.config.save() {
            Ok(()) => {
                self.baseline = self.config.clone();
                self.dirty = false;
                self.save_error = None;
                self.raw = TextEdit::new(self.config.to_toml().unwrap_or_default());
            }
            Err(error) => {
                self.save_error = Some(error.to_string());
            }
        }
    }

    pub fn discard_changes(&mut self) {
        self.config = self.baseline.clone();
        self.sync_edits();
        self.raw = TextEdit::new(self.config.to_toml().unwrap_or_default());
        self.dirty = false;
        self.save_error = None;
        let active = self.active_services();
        self.services.retain(|id| active.contains(id));
        self.sync_pending_outcomes();
    }

    pub fn reload_config(&mut self) {
        match Config::load() {
            Ok(config) => {
                self.config = config;
                self.baseline = self.config.clone();
                self.sync_edits();
                self.raw = TextEdit::new(self.config.to_toml().unwrap_or_default());
                self.dirty = false;
                self.save_error = None;
                self.load_error = None;
                let active = self.active_services();
                self.services.retain(|id| active.contains(id));
                if self.services.is_empty() {
                    self.services = active;
                }
                self.set_notice("Config reloaded.");
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
            }
        }
    }

    pub fn apply_raw(&mut self) {
        match toml::from_str::<Config>(&self.raw.text) {
            Ok(config) => {
                self.config = config;
                self.sync_edits();
                self.dirty = true;
                self.save_error = None;
                self.load_error = None;
                let active = self.active_services();
                self.services.retain(|id| active.contains(id));
                self.sync_pending_outcomes();
                self.set_notice("TOML loaded into the form. Review, then save.");
            }
            Err(error) => {
                self.save_error = Some(format!("Invalid TOML: {error}"));
            }
        }
    }

    pub fn reset_raw(&mut self) {
        self.raw = TextEdit::new(self.config.to_toml().unwrap_or_default());
        self.save_error = None;
    }

    fn sync_edits(&mut self) {
        for key in ConfigKey::ALL {
            if let Some(edit) = self.edits.get_mut(key) {
                edit.replace_text(self.config.get_value(key));
            }
        }
    }

    fn sync_pending_outcomes(&mut self) {
        let services = self.services.clone();
        for item in &mut self.queue {
            if item.status == ItemStatus::Pending {
                item.outcomes = services
                    .iter()
                    .copied()
                    .map(ServiceOutcome::pending)
                    .collect();
            }
        }
    }
}

fn outcome_mut(
    queue: &mut [QueueItem],
    item: u64,
    service: UploaderId,
) -> Option<&mut ServiceOutcome> {
    queue
        .iter_mut()
        .find(|entry| entry.id == item)?
        .outcomes
        .iter_mut()
        .find(|outcome| outcome.service == service)
}

fn set_strategy(preprocess: &mut PreprocessConfig, strategy: PreprocessStrategy, on: bool) {
    if on {
        if !preprocess
            .rules
            .iter()
            .any(|rule| rule.strategy == strategy)
        {
            preprocess.rules.push(PreprocessRule::new(strategy));
        }
    } else {
        preprocess.rules.retain(|rule| rule.strategy != strategy);
    }
}

pub fn enabled_services(config: &Config) -> Vec<UploaderId> {
    UploaderId::ALL
        .iter()
        .copied()
        .filter(|id| {
            !config
                .global
                .disabled_uploaders
                .iter()
                .any(|name| name == id.name())
        })
        .collect()
}

pub fn active_services(config: &Config) -> Vec<UploaderId> {
    enabled_services(config)
        .into_iter()
        .filter(|id| missing_credential(config, *id).is_none())
        .collect()
}

pub fn missing_credential(config: &Config, id: UploaderId) -> Option<&'static str> {
    let empty = |key: ConfigKey| config.get_value(&key).trim().is_empty();
    match id.name() {
        "bunkr" if empty(ConfigKey::BunkrToken) => Some("API token"),
        "pixeldrain" if empty(ConfigKey::PixeldrainToken) => Some("API key"),
        "goonbox" if empty(ConfigKey::GoonboxUsername) => Some("username"),
        "goonbox" if empty(ConfigKey::GoonboxPassword) => Some("password"),
        _ => None,
    }
}

pub fn poll(app: &mut App, _clock: f64, _tx: &mut Tx) -> bool {
    app.poll_notice()
}
