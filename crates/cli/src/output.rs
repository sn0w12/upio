use tiny_table::{Align, Cell, Column, Table};

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
