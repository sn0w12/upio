#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod coss;
mod theme;
mod ui;
mod window_theme;
mod worker;

use std::path::PathBuf;

use rune::{App, AppSpec, Cx, El};
use upio::registry::UploaderId;
use upio_config::ConfigKey;

use crate::app::{
    App as State, ItemStatus, OutcomeState, Page, QueueItem, ServiceOutcome, SettingsSection,
};

fn view(state: &State, cx: &mut Cx<State>) -> El {
    ui::view(state, cx)
}

fn main() {
    let _ = rune::set_app_icon(include_bytes!("../assets/brand/logo.png").to_vec());

    let mut spec = AppSpec::new("upio", (1040.0, 720.0), State::new, view);
    spec.min_size = Some((880.0, 600.0));
    spec.scenes = SCENES;
    spec.poll = Some(app::poll);

    let app = App::new(spec);
    rune::set_app_id("upio");
    worker::start(app.handle());
    app.run();
}

type Scene = (&'static str, fn(&mut State));

const SCENES: &[Scene] = &[
    ("upload", |_| {}),
    ("upload-queue", queue),
    ("upload-services", |s| {
        s.open = Some("destinations".to_string());
    }),
    ("upload-details", |s| {
        queue(s);
        s.open = Some("queue-1".to_string());
    }),
    ("upload-pending", |s| {
        queue(s);
        s.queue[2].status = ItemStatus::Pending;
        for outcome in &mut s.queue[2].outcomes {
            outcome.state = OutcomeState::Waiting;
            outcome.progress = None;
        }
        s.open = Some("queue-3".to_string());
    }),
    ("upload-plain", |s| {
        for key in [
            ConfigKey::BunkrToken,
            ConfigKey::PixeldrainToken,
            ConfigKey::GoonboxUsername,
        ] {
            let _ = s.config.set_value(&key, "");
        }
        s.services = s.active_services();
    }),
    ("settings-setup", |s| {
        s.page = Page::Settings;
        s.settings_section = SettingsSection::Services;
        s.settings_service = UploaderId::from_name("bunkr").unwrap();
        let _ = s.config.set_value(&ConfigKey::BunkrToken, "");
    }),
    ("settings-appearance", |s| {
        s.page = Page::Settings;
        s.settings_section = SettingsSection::Appearance;
    }),
    ("settings-services", |s| {
        s.page = Page::Settings;
        s.settings_section = SettingsSection::Services;
    }),
    ("settings-tools", |s| {
        s.page = Page::Settings;
        s.settings_section = SettingsSection::Tools;
    }),
    ("settings-advanced", |s| {
        s.page = Page::Settings;
        s.settings_section = SettingsSection::Advanced;
    }),
];

fn queue(s: &mut State) {
    let services = s.services.clone();
    let outcome = |service: UploaderId, state: OutcomeState| ServiceOutcome {
        service,
        state,
        progress: None,
        urls: Vec::new(),
        error: None,
    };

    let mut first: Vec<ServiceOutcome> = services
        .iter()
        .copied()
        .map(|id| outcome(id, OutcomeState::Done))
        .collect();
    for item in &mut first {
        item.urls = vec![format!("https://example.test/{}", item.service.name())];
    }
    let mut second: Vec<ServiceOutcome> = services
        .iter()
        .copied()
        .map(|id| outcome(id, OutcomeState::Waiting))
        .collect();
    if let Some(item) = second.first_mut() {
        item.state = OutcomeState::Failed;
        item.error = Some("token rejected".to_string());
    }

    s.queue = vec![
        QueueItem {
            id: 1,
            path: PathBuf::from("holiday-clip.mp4"),
            size: 84_000_000,
            status: ItemStatus::Done,
            outcomes: first,
        },
        QueueItem {
            id: 2,
            path: PathBuf::from("interview-audio.flac"),
            size: 310_000_000,
            status: ItemStatus::Failed,
            outcomes: second,
        },
        QueueItem {
            id: 3,
            path: PathBuf::from("screenshots.zip"),
            size: 12_500_000,
            status: ItemStatus::Working,
            outcomes: services
                .iter()
                .copied()
                .map(|id| outcome(id, OutcomeState::Waiting))
                .collect(),
        },
    ];
    if let Some(item) = s.queue[2].outcomes.first_mut() {
        item.state = OutcomeState::Uploading;
        item.progress = Some((7_400_000, 12_500_000));
    }
}
