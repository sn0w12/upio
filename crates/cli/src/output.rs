use std::path::Path;

use tiny_table::{Align, Cell, Column, Table};
use upio::download::DownloadResult;
use upio::filesize::FileSize;

/// Results of one uploader's batch, ready for display.
#[derive(Debug)]
pub struct UploaderResult {
    pub name: String,
    pub rows: Vec<Row>,
}

/// One file's outcome in a batch.
#[derive(Debug)]
pub struct Row {
    pub file: String,
    pub url: String,
    pub status: String,
}

impl Row {
    /// A row that carries no file/url, just a message (e.g. "no token").
    pub fn message(status: impl Into<String>) -> Self {
        Self {
            file: String::new(),
            url: String::new(),
            status: status.into(),
        }
    }

    pub fn from_result(result: upio::pipeline::FileResult) -> Self {
        Self {
            file: result.file,
            url: if result.urls.is_empty() {
                String::new()
            } else {
                result.urls.join(", ")
            },
            status: result.error.unwrap_or_else(|| "ok".to_string()),
        }
    }
}

/// Print upload results as a table (grouped by uploader) plus a per-uploader
/// summary line.
pub fn print_upload(results: &[UploaderResult]) {
    let mut table = Table::with_columns(vec![
        Column::new("file").align(Align::Left),
        Column::new("url").align(Align::Left),
        Column::new("status").align(Align::Left),
    ]);

    for result in results {
        table.add_section(&result.name).align(Align::Left);

        if result.rows.is_empty() {
            table.add_row(vec![
                Cell::new("-"),
                Cell::new("-"),
                Cell::new("no files").yellow(),
            ]);
        } else {
            for row in &result.rows {
                let status = if row.status == "ok" {
                    Cell::new(&row.status).green()
                } else {
                    Cell::new(&row.status).red()
                };
                table.add_row(vec![Cell::new(&row.file), Cell::new(&row.url), status]);
            }
        }
    }

    table.print();

    for result in results {
        let ok = result.rows.iter().filter(|row| row.status == "ok").count();
        let failed = result.rows.len() - ok;
        println!("{}: {} uploaded, {} failed", result.name, ok, failed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadOutcome {
    Downloaded,
    Skipped,
    Planned,
    Failed,
}

pub struct DownloadRow {
    pub file: String,
    pub destination: String,
    pub status: String,
    pub outcome: DownloadOutcome,
}

impl DownloadRow {
    pub fn failure(status: impl Into<String>) -> Self {
        Self {
            file: "-".to_string(),
            destination: "-".to_string(),
            status: status.into(),
            outcome: DownloadOutcome::Failed,
        }
    }

    pub fn planned(destination: &Path) -> Self {
        Self {
            file: file_name(destination),
            destination: destination.display().to_string(),
            status: "planned".to_string(),
            outcome: DownloadOutcome::Planned,
        }
    }

    pub fn from_result(result: DownloadResult) -> Self {
        let DownloadResult {
            destination,
            bytes,
            skipped,
            error,
            ..
        } = result;
        let file = file_name(&destination);
        let destination = destination.display().to_string();

        match (error, skipped) {
            (Some(error), _) => Self {
                file,
                destination,
                status: error,
                outcome: DownloadOutcome::Failed,
            },
            (None, true) => Self {
                file,
                destination,
                status: "skipped".to_string(),
                outcome: DownloadOutcome::Skipped,
            },
            (None, false) => Self {
                file,
                destination,
                status: format!("ok ({})", FileSize::from_bytes(bytes).to_human_string()),
                outcome: DownloadOutcome::Downloaded,
            },
        }
    }
}

pub struct DownloadBatch {
    pub label: String,
    pub rows: Vec<DownloadRow>,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Print one row per file, then a downloaded/skipped/failed summary.
pub fn print_download(batches: &[DownloadBatch]) {
    let mut table = Table::with_columns(vec![
        Column::new("source").align(Align::Left),
        Column::new("file").align(Align::Left),
        Column::new("destination").align(Align::Left),
        Column::new("status").align(Align::Left),
    ]);

    for batch in batches {
        if batch.rows.is_empty() {
            table.add_row(vec![
                Cell::new(&batch.label),
                Cell::new("-"),
                Cell::new("-"),
                Cell::new("no files").yellow(),
            ]);
        } else {
            for row in &batch.rows {
                let status = match row.outcome {
                    DownloadOutcome::Downloaded => Cell::new(&row.status).green(),
                    DownloadOutcome::Skipped | DownloadOutcome::Planned => {
                        Cell::new(&row.status).yellow()
                    }
                    DownloadOutcome::Failed => Cell::new(&row.status).red(),
                };
                table.add_row(vec![
                    Cell::new(&batch.label),
                    Cell::new(&row.file),
                    Cell::new(&row.destination),
                    status,
                ]);
            }
        }
    }

    table.print();

    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut planned = 0usize;
    let mut failed = 0usize;
    for row in batches.iter().flat_map(|batch| &batch.rows) {
        match row.outcome {
            DownloadOutcome::Downloaded => downloaded += 1,
            DownloadOutcome::Skipped => skipped += 1,
            DownloadOutcome::Planned => planned += 1,
            DownloadOutcome::Failed => failed += 1,
        }
    }
    if planned > 0 {
        println!("{} planned", planned);
    }
    println!(
        "{} downloaded, {} skipped, {} failed",
        downloaded, skipped, failed
    );
}

/// Print a table of `[name, status, detail]` rows with colored statuses.
///
/// `status` is colored using the value itself: `enabled`/`available` are shown
/// in green, `missing`/`disabled` in yellow, and anything else in red.
pub fn print_table(rows: &[(&str, &str, String)]) {
    let mut table = Table::with_columns(vec![
        Column::new("name").align(Align::Left),
        Column::new("status").align(Align::Left),
        Column::new("detail").align(Align::Left),
    ]);

    for (name, status, detail) in rows {
        let colored = match *status {
            "enabled" | "available" => Cell::new(*status).green(),
            "missing" | "disabled" => Cell::new(*status).yellow(),
            _ => Cell::new(*status).red(),
        };
        table.add_row(vec![Cell::new(*name), colored, Cell::new(detail.clone())]);
    }

    table.print();
}
