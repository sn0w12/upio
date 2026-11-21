//! The CLI's report model and its two renderings: a live tree per transfer and
//! the final tree printed once a command finishes.

use std::path::Path;
use std::time::{Duration, Instant};

use upio::filesize::FileSize;

use crate::style::{self, Screen, Tone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Waiting,
    Running,
    Ok,
    Skipped,
    Planned,
    Failed,
    Notice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Resolving,
    Running,
    Done,
    Failed,
    Notice,
}

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub detail: String,
    pub message: String,
    pub outcome: Outcome,
    pub bytes: u64,
    pub transferred: u64,
    pub total: u64,
    pub started: Option<Instant>,
    pub duration: Option<Duration>,
}

impl Item {
    pub fn waiting(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
            message: String::new(),
            outcome: Outcome::Waiting,
            bytes: 0,
            transferred: 0,
            total: 0,
            started: None,
            duration: None,
        }
    }

    pub fn notice(message: impl Into<String>) -> Self {
        Self {
            name: String::new(),
            detail: String::new(),
            message: message.into(),
            outcome: Outcome::Notice,
            bytes: 0,
            transferred: 0,
            total: 0,
            started: None,
            duration: None,
        }
    }

    pub fn skipped(name: impl Into<String>, detail: impl Into<String>, bytes: u64) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
            message: String::new(),
            outcome: Outcome::Skipped,
            bytes,
            transferred: bytes,
            total: bytes,
            started: None,
            duration: None,
        }
    }

    pub fn planned(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
            message: String::new(),
            outcome: Outcome::Planned,
            bytes: 0,
            transferred: 0,
            total: 0,
            started: None,
            duration: None,
        }
    }

    pub fn done(&mut self, bytes: u64) {
        self.outcome = Outcome::Ok;
        self.bytes = bytes;
        self.transferred = bytes;
        self.total = bytes;
        self.duration = self.started.map(|started| started.elapsed());
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.outcome = Outcome::Failed;
        self.message = message.into();
        self.duration = self.started.map(|started| started.elapsed());
    }

    fn elapsed(&self) -> Option<Duration> {
        self.duration
            .or_else(|| self.started.map(|started| started.elapsed()))
    }
}

#[derive(Clone)]
pub struct UploaderResult {
    pub name: String,
    pub phase: Phase,
    pub items: Vec<Item>,
    pub started: Instant,
    pub duration: Option<Duration>,
}

impl UploaderResult {
    pub fn running(name: impl Into<String>, files: &[String]) -> Self {
        Self {
            name: name.into(),
            phase: Phase::Running,
            items: files
                .iter()
                .cloned()
                .map(|file| Item::waiting(file, ""))
                .collect(),
            started: Instant::now(),
            duration: None,
        }
    }

    pub fn notice(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            phase: Phase::Notice,
            items: vec![Item::notice(message)],
            started: Instant::now(),
            duration: None,
        }
    }

    pub fn finish(&mut self) {
        self.duration = Some(self.started.elapsed());
        self.phase = if self
            .items
            .iter()
            .any(|item| item.outcome == Outcome::Failed)
        {
            Phase::Failed
        } else if self.phase == Phase::Notice {
            Phase::Notice
        } else {
            Phase::Done
        };
    }
}

#[derive(Clone)]
pub struct DownloadBatch {
    pub label: String,
    pub phase: Phase,
    pub items: Vec<Item>,
    pub started: Instant,
    pub duration: Option<Duration>,
}

impl DownloadBatch {
    pub fn resolving(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            phase: Phase::Resolving,
            items: Vec::new(),
            started: Instant::now(),
            duration: None,
        }
    }

    pub fn failed(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            phase: Phase::Failed,
            items: vec![Item::notice(message)],
            started: Instant::now(),
            duration: None,
        }
    }

    pub fn finish(&mut self) {
        self.duration = Some(self.started.elapsed());
        self.phase = if self
            .items
            .iter()
            .any(|item| item.outcome == Outcome::Failed)
        {
            Phase::Failed
        } else {
            Phase::Done
        };
    }
}

#[derive(Clone)]
pub struct UploadView {
    pub uploaders: Vec<UploaderResult>,
}

#[derive(Clone)]
pub struct DownloadView {
    pub batches: Vec<DownloadBatch>,
}

pub fn print_upload(results: &[UploaderResult]) {
    let screen = style::screen();
    for line in upload_lines(results, screen, false) {
        println!("{}", line.trim_end());
    }
}

pub fn print_download(batches: &[DownloadBatch]) {
    let screen = style::screen();
    for line in download_lines(batches, screen, false) {
        println!("{}", line.trim_end());
    }
}

pub fn print_mirror(download: &DownloadBatch, uploads: &[UploaderResult]) {
    let screen = style::screen();
    for line in mirror_lines(download, uploads, screen) {
        println!("{}", line.trim_end());
    }
}

pub fn upload_lines(results: &[UploaderResult], screen: Screen, live: bool) -> Vec<String> {
    let elapsed = results
        .iter()
        .map(|result| result.started)
        .min()
        .map(|started| started.elapsed())
        .unwrap_or_default();
    let files = unique_files(results);

    let mut lines = Vec::new();
    let files_label = format!("{} {}", files, plural(files, "file", "files"));
    let meta = if results.len() > 1 {
        format!(
            "{files_label} · {} {}",
            results.len(),
            plural(results.len(), "service", "services")
        )
    } else {
        files_label
    };
    lines.push(header("upload", &meta, elapsed, screen));
    lines.push(String::new());

    let mut budget = budget(screen, live);
    for result in results {
        if budget == 0 {
            break;
        }
        upload_section(&mut lines, result, &result.name, live, screen, &mut budget);
    }

    if !results.is_empty() {
        lines.push(rule(screen));
    }
    lines.push(summary(upload_counts(results, live), screen));
    lines
}

pub fn download_lines(batches: &[DownloadBatch], screen: Screen, live: bool) -> Vec<String> {
    let elapsed = batches
        .iter()
        .map(|batch| batch.started)
        .min()
        .map(|started| started.elapsed())
        .unwrap_or_default();
    let files = batches
        .iter()
        .flat_map(|batch| &batch.items)
        .filter(|item| !item.name.is_empty())
        .count();

    let mut lines = Vec::new();
    let files_label = format!("{} {}", files, plural(files, "file", "files"));
    let meta = if batches.len() > 1 {
        format!(
            "{files_label} · {} {}",
            batches.len(),
            plural(batches.len(), "share", "shares")
        )
    } else {
        files_label
    };
    lines.push(header("download", &meta, elapsed, screen));
    lines.push(String::new());

    let mut budget = budget(screen, live);
    for batch in batches {
        if budget == 0 {
            break;
        }
        download_section(&mut lines, batch, &batch.label, live, screen, &mut budget);
    }

    if !batches.is_empty() {
        lines.push(rule(screen));
    }
    lines.push(summary(download_counts(batches, live), screen));
    lines
}

pub fn mirror_lines(
    download: &DownloadBatch,
    uploads: &[UploaderResult],
    screen: Screen,
) -> Vec<String> {
    let elapsed = download.started.elapsed();
    let files = download
        .items
        .iter()
        .filter(|item| !item.name.is_empty())
        .count();
    let files_label = format!("{} {}", files, plural(files, "file", "files"));
    let meta = if uploads.is_empty() {
        files_label
    } else {
        format!(
            "{files_label} · {} {}",
            uploads.len(),
            plural(uploads.len(), "service", "services")
        )
    };

    let mut lines = vec![header("mirror", &meta, elapsed, screen), String::new()];
    let mut budget = usize::MAX;

    let title = format!("download · {}", download.label);
    download_section(&mut lines, download, &title, false, screen, &mut budget);

    if !uploads.is_empty() {
        lines.push(format!("  {}", style::paint("↓", Tone::Branch)));
        lines.push(String::new());
        for upload in uploads {
            if budget == 0 {
                break;
            }
            let title = format!("upload · {}", upload.name);
            upload_section(&mut lines, upload, &title, false, screen, &mut budget);
        }
    }

    lines.push(rule(screen));
    lines.push(summary(mirror_counts(download, uploads), screen));
    lines
}

fn upload_section(
    lines: &mut Vec<String>,
    result: &UploaderResult,
    title: &str,
    live: bool,
    screen: Screen,
    budget: &mut usize,
) {
    lines.push(section_header(
        title,
        result.phase,
        upload_meta(result, live),
        screen,
    ));
    *budget = budget.saturating_sub(1);

    let mut omitted = 0;
    for (index, item) in result.items.iter().enumerate() {
        let last = index + 1 == result.items.len();
        if *budget == 0 {
            omitted += 1;
            continue;
        }
        let detail = render_item(lines, item, last, Kind::Upload, live, screen);
        *budget = budget.saturating_sub(detail);
    }
    if omitted > 0 {
        lines.push(overflow(omitted, "file"));
    }
    lines.push(String::new());
}

fn download_section(
    lines: &mut Vec<String>,
    batch: &DownloadBatch,
    title: &str,
    live: bool,
    screen: Screen,
    budget: &mut usize,
) {
    lines.push(section_header(
        title,
        batch.phase,
        download_meta(batch, live),
        screen,
    ));
    *budget = budget.saturating_sub(1);

    let mut omitted = 0;
    for (index, item) in batch.items.iter().enumerate() {
        let last = index + 1 == batch.items.len();
        if *budget == 0 {
            omitted += 1;
            continue;
        }
        let detail = render_item(lines, item, last, Kind::Download, live, screen);
        *budget = budget.saturating_sub(detail);
    }
    if omitted > 0 {
        lines.push(overflow(omitted, "file"));
    }
    lines.push(String::new());
}

enum Kind {
    Upload,
    Download,
}

fn render_item(
    lines: &mut Vec<String>,
    item: &Item,
    last: bool,
    kind: Kind,
    live: bool,
    screen: Screen,
) -> usize {
    let connector = if last { style::ELBOW } else { style::TEE };
    let prefix = format!("  {} ", style::paint(connector, Tone::Branch));
    let wrap = screen.width + 1;

    if item.name.is_empty() {
        let (glyph, tone) = marker(item);
        let line = format!(
            "{prefix}{} {}",
            style::paint(glyph, tone),
            style::paint(&item.message, tone)
        );
        let rows = style::rows(&line, wrap);
        lines.push(line);
        return rows;
    }

    let (glyph, marker_tone) = marker(item);
    let name_tone = match item.outcome {
        Outcome::Waiting | Outcome::Skipped | Outcome::Planned | Outcome::Ok => Tone::Muted,
        Outcome::Running => Tone::Strong,
        Outcome::Failed => Tone::Bad,
        Outcome::Notice => Tone::Warn,
    };
    let name = match item.outcome {
        Outcome::Skipped => format!("{}  skipped", item.name),
        Outcome::Planned => format!("{}  planned", item.name),
        _ => item.name.clone(),
    };

    let columns = item_columns(item, live, screen);
    let line = row(
        &prefix,
        &style::paint(glyph, marker_tone),
        &name,
        name_tone,
        matches!(kind, Kind::Upload),
        columns,
        screen.width,
    );
    lines.push(line);

    let detail = match item.outcome {
        Outcome::Failed => Some((item.message.as_str(), Tone::Bad)),
        Outcome::Notice => None,
        _ if item.detail.is_empty() => None,
        _ => Some((
            item.detail.as_str(),
            match kind {
                Kind::Upload => Tone::Accent,
                Kind::Download => Tone::Muted,
            },
        )),
    };

    let continuation = if last {
        "         ".to_string()
    } else {
        format!("  {}      ", style::paint(style::PIPE, Tone::Branch))
    };

    let Some((text, tone)) = detail.filter(|(text, _)| !text.is_empty()) else {
        if !live {
            return 1;
        }
        lines.push(format!(
            "{continuation}{}",
            style::paint("→ …", Tone::Muted)
        ));
        return 2;
    };

    let line = format!("{continuation}{}", style::paint(format!("→ {text}"), tone));
    let rows = 1 + style::rows(&line, wrap);
    lines.push(line);
    rows
}

fn row(
    prefix: &str,
    glyph: &str,
    name: &str,
    name_tone: Tone,
    tail: bool,
    columns: Vec<(String, usize)>,
    width: usize,
) -> String {
    let fixed = style::visible_width(prefix) + 4;
    let columns_width: usize = columns.iter().map(|(_, width)| width).sum();
    let name_width = width.saturating_sub(fixed + columns_width).max(1);
    let name = if tail {
        style::truncate_tail(name, name_width)
    } else {
        style::truncate(name, name_width)
    };
    let mut out = format!("{prefix}{glyph} ");
    out.push_str(&style::paint(&name, name_tone));
    out.push_str(&" ".repeat(name_width.saturating_sub(style::visible_width(&name))));
    for (column, _) in columns {
        out.push_str(&column);
    }
    out
}

fn item_columns(item: &Item, live: bool, screen: Screen) -> Vec<(String, usize)> {
    let wide = screen.width >= 88;
    let medium = screen.width >= 58;
    let narrow = screen.width >= 44;

    let streaming = live
        && matches!(
            item.outcome,
            Outcome::Running | Outcome::Waiting | Outcome::Ok
        );

    if streaming {
        let fraction = if item.total > 0 {
            item.transferred as f64 / item.total as f64
        } else if item.outcome == Outcome::Ok {
            1.0
        } else {
            0.0
        };
        let mut columns = Vec::new();
        if wide {
            columns.push(column(
                style::bar(fraction, 14, Tone::Good),
                14,
                Tone::Plain,
            ));
        }
        if medium {
            columns.push(column(percent(item), 4, Tone::Warn));
        }
        if narrow {
            columns.push(column(bytes_live(item), 16, Tone::Plain));
        }
        if wide {
            columns.push(column(
                item.elapsed().map(human_duration).unwrap_or_default(),
                7,
                Tone::Muted,
            ));
        }
        return columns;
    }

    let mut columns = Vec::new();
    if narrow {
        columns.push(column(bytes_final(item), 9, Tone::Muted));
    }
    if medium {
        columns.push(column(
            item.elapsed().map(human_duration).unwrap_or_default(),
            7,
            Tone::Muted,
        ));
    }
    if wide {
        columns.push(column(rate(item).unwrap_or_default(), 10, Tone::Muted));
    }
    columns
}

fn column(text: impl AsRef<str>, width: usize, tone: Tone) -> (String, usize) {
    let text = style::truncate(text.as_ref(), width);
    (
        format!("  {}", style::pad_left(&style::paint(&text, tone), width)),
        width + 2,
    )
}

fn percent(item: &Item) -> String {
    if item.total == 0 {
        return String::new();
    }
    let percent = (item.transferred as f64 / item.total as f64 * 100.0).round() as u64;
    format!("{percent}%")
}

fn bytes_live(item: &Item) -> String {
    if item.outcome == Outcome::Ok && item.bytes > 0 {
        return FileSize::from_bytes(item.bytes).to_human_string();
    }
    if item.total > 0 {
        format!(
            "{}/{}",
            FileSize::from_bytes(item.transferred).to_human_string(),
            FileSize::from_bytes(item.total).to_human_string()
        )
    } else if item.transferred > 0 {
        FileSize::from_bytes(item.transferred).to_human_string()
    } else {
        String::new()
    }
}

fn bytes_final(item: &Item) -> String {
    let bytes = if item.bytes > 0 {
        item.bytes
    } else {
        item.total
    };
    if bytes > 0 {
        FileSize::from_bytes(bytes).to_human_string()
    } else {
        String::new()
    }
}

fn rate(item: &Item) -> Option<String> {
    if item.outcome != Outcome::Ok {
        return None;
    }
    let elapsed = item.elapsed()?;
    if elapsed.as_millis() < 500 || item.bytes == 0 {
        return None;
    }
    let bytes_per_second = (item.bytes as f64 / elapsed.as_secs_f64()).round() as u64;
    Some(format!(
        "{}/s",
        FileSize::from_bytes(bytes_per_second).to_human_string()
    ))
}

fn marker(item: &Item) -> (String, Tone) {
    match item.outcome {
        Outcome::Waiting => ("[ ]".to_string(), Tone::Muted),
        Outcome::Running => (
            format!(
                "[{}]",
                style::spinner(
                    item.started
                        .map(|started| started.elapsed())
                        .unwrap_or_default()
                )
            ),
            Tone::Warn,
        ),
        Outcome::Ok => ("[√]".to_string(), Tone::Good),
        Outcome::Skipped => ("[-]".to_string(), Tone::Muted),
        Outcome::Planned => ("[·]".to_string(), Tone::Muted),
        Outcome::Failed => ("[×]".to_string(), Tone::Bad),
        Outcome::Notice => ("[!]".to_string(), Tone::Warn),
    }
}

fn header(command: &str, meta: &str, elapsed: Duration, screen: Screen) -> String {
    let left = format!(
        "  {} {} {}",
        style::paint("upio", Tone::Strong),
        style::paint(command, Tone::Plain),
        style::paint(format!("· {meta}"), Tone::Muted),
    );
    style::spread(
        left,
        style::paint(human_duration(elapsed), Tone::Plain),
        screen.width,
    )
}

fn section_header(label: &str, phase: Phase, meta: String, screen: Screen) -> String {
    let (root, tone) = match phase {
        Phase::Resolving | Phase::Running => (style::ROOT, Tone::Warn),
        Phase::Done => (style::ROOT, Tone::Good),
        Phase::Failed => (style::ROOT, Tone::Bad),
        Phase::Notice => (style::ROOT, Tone::Warn),
    };
    let left = format!(
        "  {} {}",
        style::paint(root, tone),
        style::paint(label, Tone::Strong)
    );
    style::spread(left, meta, screen.width)
}

fn upload_meta(result: &UploaderResult, live: bool) -> String {
    if result.phase == Phase::Notice {
        return String::new();
    }
    let total = result
        .items
        .iter()
        .filter(|item| item.outcome != Outcome::Notice)
        .count();
    let done = result
        .items
        .iter()
        .filter(|item| matches!(item.outcome, Outcome::Ok | Outcome::Failed))
        .count();
    let failed = result
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Failed)
        .count();
    let elapsed = result.duration.unwrap_or_else(|| result.started.elapsed());

    if live && result.phase == Phase::Running {
        let mut parts = vec![style::paint(format!("{done} of {total}"), Tone::Plain)];
        if failed > 0 {
            parts.push(style::paint(format!("{failed} failed"), Tone::Bad));
        }
        parts.push(style::paint(human_duration(elapsed), Tone::Muted));
        return join(parts);
    }

    let ok = result
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Ok)
        .count();
    let mut parts = Vec::new();
    if ok > 0 {
        parts.push(style::paint(format!("{ok} uploaded"), Tone::Good));
    }
    if failed > 0 {
        parts.push(style::paint(format!("{failed} failed"), Tone::Bad));
    }
    if parts.is_empty() {
        parts.push(style::paint("no files", Tone::Muted));
    }
    parts.push(style::paint(human_duration(elapsed), Tone::Muted));
    join(parts)
}

fn download_meta(batch: &DownloadBatch, live: bool) -> String {
    if batch.phase == Phase::Failed
        && batch
            .items
            .iter()
            .all(|item| item.outcome == Outcome::Notice)
    {
        return String::new();
    }
    if batch.phase == Phase::Resolving {
        return style::paint("resolving…", Tone::Warn);
    }

    let total = batch.items.len();
    let done = batch
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.outcome,
                Outcome::Ok | Outcome::Skipped | Outcome::Failed
            )
        })
        .count();
    let failed = batch
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Failed)
        .count();
    let elapsed = batch.duration.unwrap_or_else(|| batch.started.elapsed());

    if live {
        let mut parts = vec![style::paint(format!("{done} of {total}"), Tone::Plain)];
        if failed > 0 {
            parts.push(style::paint(format!("{failed} failed"), Tone::Bad));
        }
        parts.push(style::paint(human_duration(elapsed), Tone::Muted));
        return join(parts);
    }

    let bytes: u64 = batch
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Ok)
        .map(|item| item.bytes)
        .sum();
    let mut parts = Vec::new();
    if bytes > 0 {
        parts.push(style::paint(
            FileSize::from_bytes(bytes).to_human_string(),
            Tone::Muted,
        ));
    }
    parts.push(style::paint(human_duration(elapsed), Tone::Muted));
    join(parts)
}

fn upload_counts(results: &[UploaderResult], live: bool) -> Vec<String> {
    let items = || results.iter().flat_map(|result| &result.items);
    counts(
        Tally {
            done: items().filter(|item| item.outcome == Outcome::Ok).count(),
            running: items()
                .filter(|item| item.outcome == Outcome::Running)
                .count(),
            waiting: items()
                .filter(|item| item.outcome == Outcome::Waiting)
                .count(),
            skipped: items()
                .filter(|item| item.outcome == Outcome::Skipped)
                .count(),
            planned: items()
                .filter(|item| item.outcome == Outcome::Planned)
                .count(),
            failed: items()
                .filter(|item| item.outcome == Outcome::Failed)
                .count(),
        },
        live,
        "uploaded",
    )
}

fn download_counts(batches: &[DownloadBatch], live: bool) -> Vec<String> {
    let items = || batches.iter().flat_map(|batch| &batch.items);
    counts(
        Tally {
            done: items().filter(|item| item.outcome == Outcome::Ok).count(),
            running: items()
                .filter(|item| item.outcome == Outcome::Running)
                .count(),
            waiting: items()
                .filter(|item| item.outcome == Outcome::Waiting)
                .count(),
            skipped: items()
                .filter(|item| item.outcome == Outcome::Skipped)
                .count(),
            planned: items()
                .filter(|item| item.outcome == Outcome::Planned)
                .count(),
            failed: items()
                .filter(|item| item.outcome == Outcome::Failed)
                .count(),
        },
        live,
        "downloaded",
    )
}

fn mirror_counts(download: &DownloadBatch, uploads: &[UploaderResult]) -> Vec<String> {
    let downloaded = download
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Ok)
        .count();
    let uploaded = uploads
        .iter()
        .flat_map(|result| &result.items)
        .filter(|item| item.outcome == Outcome::Ok)
        .count();
    let skipped = download
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Skipped)
        .count();
    let planned = download
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Planned)
        .count();
    let failed = download
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Failed)
        .count()
        + uploads
            .iter()
            .flat_map(|result| &result.items)
            .filter(|item| item.outcome == Outcome::Failed)
            .count();

    let mut parts = vec![paint_count(
        downloaded,
        format!("{downloaded} downloaded"),
        Tone::Good,
    )];
    if uploaded > 0 {
        parts.push(paint_count(
            uploaded,
            format!("{uploaded} uploaded"),
            Tone::Good,
        ));
    }
    if planned > 0 {
        parts.push(paint_count(
            planned,
            format!("{planned} planned"),
            Tone::Warn,
        ));
    }
    if skipped > 0 {
        parts.push(paint_count(
            skipped,
            format!("{skipped} skipped"),
            Tone::Muted,
        ));
    }
    parts.push(paint_count(failed, format!("{failed} failed"), Tone::Bad));
    parts
}

struct Tally {
    done: usize,
    running: usize,
    waiting: usize,
    skipped: usize,
    planned: usize,
    failed: usize,
}

fn counts(tally: Tally, live: bool, verb: &str) -> Vec<String> {
    let mut parts = vec![paint_count(
        tally.done,
        format!("{} {verb}", tally.done),
        Tone::Good,
    )];
    if live {
        if tally.running > 0 {
            parts.push(paint_count(
                tally.running,
                format!("{} running", tally.running),
                Tone::Warn,
            ));
        }
        if tally.waiting > 0 {
            parts.push(paint_count(
                tally.waiting,
                format!("{} waiting", tally.waiting),
                Tone::Muted,
            ));
        }
    }
    if tally.planned > 0 {
        parts.push(paint_count(
            tally.planned,
            format!("{} planned", tally.planned),
            Tone::Warn,
        ));
    }
    if tally.skipped > 0 {
        parts.push(paint_count(
            tally.skipped,
            format!("{} skipped", tally.skipped),
            Tone::Muted,
        ));
    }
    parts.push(paint_count(
        tally.failed,
        format!("{} failed", tally.failed),
        Tone::Bad,
    ));
    parts
}

fn paint_count(count: usize, text: String, tone: Tone) -> String {
    if count == 0 {
        style::paint(text, Tone::Muted)
    } else {
        style::paint(text, tone)
    }
}

fn summary(parts: Vec<String>, screen: Screen) -> String {
    style::spread(format!("  {}", join(parts)), String::new(), screen.width)
}

fn join(parts: Vec<String>) -> String {
    let separator = format!(" {} ", style::paint("·", Tone::Muted));
    parts.join(&separator)
}

fn rule(screen: Screen) -> String {
    format!("  {}", style::rule(screen.width.saturating_sub(4)))
}

fn overflow(count: usize, noun: &str) -> String {
    format!(
        "       {}",
        style::paint(
            format!(
                "… {count} more {}",
                plural(count, noun, &format!("{noun}s"))
            ),
            Tone::Muted
        )
    )
}

fn budget(screen: Screen, live: bool) -> usize {
    if live {
        screen.height.saturating_sub(10)
    } else {
        usize::MAX
    }
}

fn unique_files(results: &[UploaderResult]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for item in results.iter().flat_map(|result| &result.items) {
        if !item.name.is_empty() {
            seen.insert(item.name.clone());
        }
    }
    seen.len()
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds < 10.0 {
        format!("{seconds:.1}s")
    } else if seconds < 60.0 {
        format!("{:.0}s", seconds)
    } else {
        let minutes = (seconds / 60.0).floor() as u64;
        let rest = (seconds - minutes as f64 * 60.0).round() as u64;
        if rest == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m {rest}s")
        }
    }
}

pub fn print_table(rows: &[(&str, &str, String)]) {
    if rows.is_empty() {
        return;
    }
    let name_width = rows
        .iter()
        .map(|(name, _, _)| style::visible_width(name))
        .max()
        .unwrap_or(0)
        .max(4);
    let status_width = rows
        .iter()
        .map(|(_, status, _)| style::visible_width(status) + 4)
        .max()
        .unwrap_or(6);
    let screen = style::screen();

    println!(
        "{}",
        format!(
            "  {}{}{}",
            style::pad_right(&style::paint("name", Tone::Muted), name_width + 2),
            style::pad_right(&style::paint("status", Tone::Muted), status_width + 2),
            style::paint("detail", Tone::Muted)
        )
        .trim_end()
    );

    for (name, status, detail) in rows {
        let (glyph, tone) = match *status {
            "enabled" | "available" => ("[√]", Tone::Good),
            "missing" | "disabled" => ("[!]", Tone::Warn),
            _ => ("[×]", Tone::Bad),
        };
        let status_cell = format!("{glyph} {status}");
        let room = screen.width.saturating_sub(name_width + status_width + 6);
        println!(
            "{}",
            format!(
                "  {}{}{}",
                style::pad_right(&style::paint(name, Tone::Plain), name_width + 2),
                style::pad_right(&style::paint(&status_cell, tone), status_width + 2),
                style::paint(style::truncate(detail, room), Tone::Muted)
            )
            .trim_end()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Screen {
        Screen {
            width: 100,
            height: 40,
        }
    }

    fn upload() -> UploaderResult {
        let mut result = UploaderResult::running("bunkr", &["clip.mp4".into(), "bad.mp4".into()]);
        result.items[0].detail = "https://bunkr.si/f/abc".into();
        result.items[0].outcome = Outcome::Ok;
        result.items[0].bytes = 12_400_000;
        result.items[0].duration = Some(Duration::from_millis(3100));
        result.items[1].outcome = Outcome::Failed;
        result.items[1].message = "413 Request Entity Too Large".into();
        result.items[1].duration = Some(Duration::from_millis(400));
        result.finish();
        result
    }

    #[test]
    fn upload_report_keeps_the_tree_and_counts() {
        let lines = upload_lines(&[upload()], screen(), false).join("\n");
        assert!(lines.contains("upio upload"));
        assert!(lines.contains("◆ bunkr"));
        assert!(lines.contains("[√] clip.mp4"));
        assert!(lines.contains("[×] bad.mp4"));
        assert!(lines.contains("1 uploaded"));
        assert!(lines.contains("1 failed"));
        assert!(lines.contains("413 Request Entity Too Large"));
    }

    #[test]
    fn narrow_screens_drop_the_optional_columns() {
        let narrow = Screen {
            width: 46,
            height: 24,
        };
        let lines = upload_lines(&[upload()], narrow, false);
        for line in &lines {
            assert!(style::visible_width(line) <= narrow.width);
        }
    }

    #[test]
    fn live_rows_keep_their_detail_line() {
        let mut waiting = UploaderResult::running("bunkr", &["clip.mp4".into(), "bad.mp4".into()]);
        let live = upload_lines(&[waiting.clone()], screen(), true);
        assert_eq!(live.iter().filter(|line| line.contains("→ …")).count(), 2);

        waiting.items[0].done(12_400_000);
        waiting.items[0].detail = "https://bunkr.si/f/abc".into();
        waiting.items[1].fail("413 Request Entity Too Large");
        waiting.finish();
        let final_lines = upload_lines(&[waiting], screen(), false);
        assert_eq!(live.len(), final_lines.len());
    }

    #[test]
    fn long_links_stay_whole_in_live_and_final_reports() {
        let url =
            "https://fileditchvids.st/alpha/579fb1498977a4854a2a/CF1334043%20%ED%95%9C%EA%B5%AD.mp4";
        let mut result = UploaderResult::running("fileditch", &["clip.mp4".into()]);
        result.items[0].done(1);
        result.items[0].detail = url.into();
        result.finish();
        let narrow = Screen {
            width: 60,
            height: 40,
        };

        let final_report = upload_lines(std::slice::from_ref(&result), narrow, false).join("\n");
        assert!(final_report.contains(url));

        let live = upload_lines(std::slice::from_ref(&result), narrow, true);
        let detail = live
            .iter()
            .find(|line| line.contains(url))
            .expect("live report keeps the whole link");
        assert!(style::visible_width(detail) > narrow.width);
    }

    #[test]
    fn live_tree_keeps_the_summary_on_screen() {
        let files = (0..12)
            .map(|index| format!("clip-{index}.mp4"))
            .collect::<Vec<_>>();
        let result = UploaderResult::running("bunkr", &files);
        let lines = upload_lines(
            &[result],
            Screen {
                width: 100,
                height: 24,
            },
            true,
        );
        assert!(lines.len() <= 23);
        assert!(lines.last().unwrap().contains("failed"));
    }

    #[test]
    fn mirror_report_joins_both_stages() {
        let mut batch = DownloadBatch::resolving("pixeldrain: /u/abc");
        batch.items = vec![Item {
            outcome: Outcome::Ok,
            bytes: 400_000_000,
            duration: Some(Duration::from_millis(10_200)),
            ..Item::waiting("clip.mp4", "downloads/clip.mp4")
        }];
        batch.finish();

        let lines = mirror_lines(&batch, &[upload()], screen()).join("\n");
        assert!(lines.contains("upio mirror"));
        assert!(lines.contains("download · pixeldrain: /u/abc"));
        assert!(lines.contains("upload · bunkr"));
        assert!(lines.contains("↓"));
        assert!(lines.contains("1 downloaded"));
        assert!(lines.contains("1 uploaded"));
    }

    #[test]
    fn download_report_marks_skips_and_totals() {
        let mut batch = DownloadBatch::resolving("pixeldrain: /u/abc");
        batch.items = vec![
            Item {
                outcome: Outcome::Ok,
                bytes: 400_000_000,
                ..Item::waiting("clip.mp4", "downloads/clip.mp4")
            },
            Item::skipped("big.zip", "downloads/big.zip", 12_000_000),
        ];
        batch.items[0].duration = Some(Duration::from_millis(10_200));
        batch.finish();

        let lines = download_lines(&[batch], screen(), false).join("\n");
        assert!(lines.contains("2 files"));
        assert!(lines.contains("[√] clip.mp4"));
        assert!(lines.contains("skipped"));
        assert!(lines.contains("1 downloaded"));
        assert!(lines.contains("1 skipped"));
        assert!(lines.contains("400.0 MB"));
    }
}
