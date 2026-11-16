//! Debug-only simulated transfers, for looking at the report without a network.

use std::time::{Duration, Instant};

use anyhow::Result;
use futures::StreamExt;

use crate::cli::DemoAction;
use crate::live::{Handle, Live};
use crate::output::{
    self, DownloadBatch, DownloadView, Item, Outcome, Phase, UploadView, UploaderResult,
};

const STEPS: u64 = 10;
const SHARES: [&str; 2] = ["pixeldrain: /u/demo123", "gofile: /d/demo456"];

const UPLOAD_FILES: [(&str, u64); 8] = [
    ("videos/launch-trailer.mp4", 48_200_000),
    ("videos/teaser-cut.mp4", 12_400_000),
    ("art/cover-final.png", 3_100_000),
    ("audio/theme.flac", 28_700_000),
    ("maps/level-01.bin", 6_800_000),
    ("docs/release-notes.txt", 18_400),
    ("archive/assets.zip", 24_500_000),
    ("clips/session-04.mp4", 9_900_000),
];

const DOWNLOAD_FILES: [(&str, u64); 8] = [
    ("clip.mp4", 400_000_000),
    ("clip2.mp4", 300_000_000),
    ("extras.zip", 12_000_000),
    ("notes.pdf", 2_400_000),
    ("cover.png", 1_800_000),
    ("broken.bin", 8_000_000),
    ("session.mkv", 850_000_000),
    ("readme.txt", 9_200),
];

pub async fn run(action: DemoAction) -> Result<()> {
    match action {
        DemoAction::Upload { files } => {
            let uploads = play_upload(upload_catalog(files)).await?;
            output::print_upload(&uploads);
        }
        DemoAction::Download { files } => {
            let batches = play_download(files, 2).await?;
            output::print_download(&batches);
        }
        DemoAction::Mirror { files } => {
            let batches = play_download(files, 1).await?;
            let batch = batches.into_iter().next().expect("one share requested");
            let files = batch
                .items
                .iter()
                .filter(|item| item.outcome == Outcome::Ok)
                .map(|item| (item.detail.clone(), item.bytes))
                .collect();
            let uploads = play_upload(files).await?;
            output::print_mirror(&batch, &uploads);
        }
    }
    Ok(())
}

fn upload_catalog(count: usize) -> Vec<(String, u64)> {
    let count = count.clamp(1, UPLOAD_FILES.len());
    UPLOAD_FILES[..count]
        .iter()
        .map(|(name, size)| ((*name).to_string(), *size))
        .collect()
}

async fn play_upload(files: Vec<(String, u64)>) -> Result<Vec<UploaderResult>> {
    let names = files
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    let live = Live::start(
        UploadView {
            uploaders: vec![
                UploaderResult::running("bunkr", &names),
                UploaderResult::running("gofile", &names),
                UploaderResult::notice("filester", "disabled uploader"),
            ],
        },
        |view, screen| output::upload_lines(&view.uploaders, screen, true),
    );
    let handle = live.handle();

    let mut tasks = Vec::new();
    for uploader in 0..2 {
        let handle = handle.clone();
        let files = files.clone();
        let count = names.len();
        let jobs = (0..count)
            .map(|file| {
                let handle = handle.clone();
                let size = files[file].1;
                let fail = uploader == 0 && file + 1 == count && count > 1;
                async move { simulate_upload(handle, uploader, file, size, fail).await }
            })
            .collect::<Vec<_>>();

        tasks.push(tokio::spawn(async move {
            futures::stream::iter(jobs).buffered(2).count().await;
            handle.update(|view| view.uploaders[uploader].finish());
        }));
    }

    for task in tasks {
        task.await?;
    }

    let view = live.view();
    live.finish();
    Ok(view.uploaders)
}

async fn play_download(count: usize, shares: usize) -> Result<Vec<DownloadBatch>> {
    let shares = shares.clamp(1, SHARES.len());
    let count = count.clamp(shares, DOWNLOAD_FILES.len());
    let base = count / shares;

    let mut batches = Vec::new();
    let mut plans = Vec::new();
    let mut start = 0;

    for (batch, label) in SHARES[..shares].iter().enumerate() {
        let end = if batch + 1 == shares {
            count
        } else {
            start + base
        };
        let mut items = Vec::new();
        let mut jobs = Vec::new();

        for (file, (name, size)) in DOWNLOAD_FILES.iter().enumerate().take(end).skip(start) {
            let name = *name;
            let size = *size;
            let destination = format!("downloads/{name}");
            let skipped = shares == 2 && batch == 0 && file + 1 == end && end - start > 1;
            if skipped {
                items.push(Item::skipped(name, destination, size));
            } else {
                let fail = shares == 2 && batch == 1 && file + 1 == count;
                jobs.push((items.len(), size, fail));
                items.push(Item::waiting(name, destination));
            }
        }

        batches.push(DownloadBatch::resolving(*label));
        plans.push((batch, items, jobs));
        start = end;
    }

    let live = Live::start(DownloadView { batches }, |view, screen| {
        output::download_lines(&view.batches, screen, true)
    });
    let handle = live.handle();

    tokio::time::sleep(Duration::from_millis(450)).await;

    let plans = plans
        .into_iter()
        .map(|(batch, items, jobs)| {
            live.update(|view| {
                let resolved = &mut view.batches[batch];
                resolved.phase = Phase::Running;
                resolved.items = items;
            });
            (batch, jobs)
        })
        .collect::<Vec<_>>();

    let mut tasks = Vec::new();
    for (batch, jobs) in plans {
        let handle = handle.clone();
        let jobs = jobs
            .into_iter()
            .map(|(item, size, fail)| {
                let handle = handle.clone();
                async move { simulate_download(handle, batch, item, size, fail).await }
            })
            .collect::<Vec<_>>();

        tasks.push(tokio::spawn(async move {
            futures::stream::iter(jobs).buffered(2).count().await;
            handle.update(|view| view.batches[batch].finish());
        }));
    }

    for task in tasks {
        task.await?;
    }

    let view = live.view();
    live.finish();
    Ok(view.batches)
}

async fn simulate_upload(
    handle: Handle<UploadView>,
    uploader: usize,
    file: usize,
    size: u64,
    fail: bool,
) {
    let started = Instant::now();
    handle.update(|view| {
        let item = &mut view.uploaders[uploader].items[file];
        item.outcome = Outcome::Running;
        item.started = Some(started);
        item.bytes = size;
        item.total = size;
    });

    for step in 1..=STEPS {
        tokio::time::sleep(delay(file, step)).await;
        if fail && step == STEPS * 2 / 3 {
            handle.update(|view| {
                view.uploaders[uploader].items[file].fail("413 Request Entity Too Large")
            });
            return;
        }
        handle.update(|view| {
            view.uploaders[uploader].items[file].transferred = size * step / STEPS;
        });
    }

    let url = if uploader == 0 {
        format!("https://bunkr.si/f/demo-{file:02}")
    } else {
        format!("https://gofile.io/d/demo-{file:02}")
    };
    handle.update(|view| {
        let item = &mut view.uploaders[uploader].items[file];
        item.done(size);
        item.detail = url;
    });
}

async fn simulate_download(
    handle: Handle<DownloadView>,
    batch: usize,
    item_index: usize,
    size: u64,
    fail: bool,
) {
    let started = Instant::now();
    handle.update(|view| {
        let item = &mut view.batches[batch].items[item_index];
        item.outcome = Outcome::Running;
        item.started = Some(started);
        item.total = size;
    });

    for step in 1..=STEPS {
        tokio::time::sleep(delay(item_index, step)).await;
        if fail && step == STEPS * 2 / 3 {
            handle.update(|view| {
                view.batches[batch].items[item_index].fail("403 Forbidden: the share link expired")
            });
            return;
        }
        handle.update(|view| {
            view.batches[batch].items[item_index].transferred = size * step / STEPS;
        });
    }

    handle.update(|view| view.batches[batch].items[item_index].done(size));
}

fn delay(seed: usize, step: u64) -> Duration {
    Duration::from_millis(55 + (seed as u64 * 17 + step * 7) % 45)
}
