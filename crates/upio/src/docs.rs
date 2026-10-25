//! Generation of the README tables.
//!
//! The services and preprocessing tables are derived from [`UploaderId`] and
//! [`STRATEGIES`], so adding a service or strategy updates the docs with
//! `cargo run -p upio --example readme` instead of a manual edit. Each table
//! sits between `BEGIN`/`END` marker comments that [`sync`] replaces in place.

use crate::preprocess::tools::Tool;
use crate::preprocess::{RuleTrigger, STRATEGIES};
use crate::registry::{Credential, UploaderId};
use crate::UploaderCapabilities;

type Render = fn() -> String;

const BLOCKS: &[(&str, Render)] = &[
    ("services", services_table),
    ("strategies", strategies_table),
];

const OPTIONAL_TOKEN_NOTE: &str = "\\* Optional. Anonymous uploads work without one, however \
                                   some functionality may be locked behind the token.";

/// An error produced while syncing a README.
#[derive(Debug, PartialEq, Eq)]
pub enum DocsError {
    /// A generated block has no `BEGIN` marker.
    MissingBlock(String),
    /// A `BEGIN` marker has no matching `END` marker.
    UnterminatedBlock(String),
}

impl std::fmt::Display for DocsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocsError::MissingBlock(block) => write!(
                f,
                "missing the '{block}' block; add <!-- BEGIN generated: {block} --> and \
                 <!-- END generated: {block} --> to the README"
            ),
            DocsError::UnterminatedBlock(block) => {
                write!(
                    f,
                    "the '{block}' block has no <!-- END generated: {block} --> marker"
                )
            }
        }
    }
}

impl std::error::Error for DocsError {}

/// Replace every generated block in `readme` with the table rendered from the
/// current code.
///
/// Returns an error when a block's markers are missing, since that block would
/// otherwise silently go stale.
pub fn sync(readme: &str) -> Result<String, DocsError> {
    let eol = line_ending(readme);
    BLOCKS
        .iter()
        .try_fold(readme.to_string(), |text, (block, render)| {
            replace_block(&text, block, &render(), eol)
        })
}

/// The services table and, when a service accepts anonymous uploads, the
/// footnote that explains the `No*` marker.
pub fn services_table() -> String {
    let mut has_optional_token = false;

    let rows = UploaderId::ALL
        .iter()
        .map(|id| {
            let info = id.info();
            let token = match info.credential {
                Credential::Token => "Yes",
                Credential::OptionalToken => {
                    has_optional_token = true;
                    "No\\*"
                }
                Credential::Account | Credential::None => "No",
            };

            vec![
                info.display_name.to_string(),
                format!("`{}`", info.crate_name),
                token.to_string(),
                yes_no(info.supports_folders),
                capabilities_label(id.capabilities()),
            ]
        })
        .collect::<Vec<_>>();

    let mut table = render_table(
        &[
            "Service",
            "Crate",
            "Token Required",
            "Folder / album",
            "Capabilities",
        ],
        &rows,
    );

    if has_optional_token {
        table.push_str("\n\n");
        table.push_str(OPTIONAL_TOKEN_NOTE);
    }

    table
}

/// The preprocessing strategy table, in canonical chain order.
pub fn strategies_table() -> String {
    let rows = STRATEGIES
        .iter()
        .enumerate()
        .map(|(index, strategy)| {
            let def = strategy.def();
            vec![
                (index + 1).to_string(),
                format!("`{}`", def.id),
                trigger_label(def.trigger),
                def.description.to_string(),
                requirements_label(def.tools),
            ]
        })
        .collect::<Vec<_>>();

    render_table(
        &["#", "Strategy", "Trigger", "What it does", "Requirements"],
        &rows,
    )
}

fn replace_block(text: &str, block: &str, content: &str, eol: &str) -> Result<String, DocsError> {
    let begin = format!("<!-- BEGIN generated: {block} -->");
    let end = format!("<!-- END generated: {block} -->");

    let start = text
        .find(&begin)
        .ok_or_else(|| DocsError::MissingBlock(block.to_string()))?;
    let body_start = start + begin.len();
    let body_end = text[body_start..]
        .find(&end)
        .map(|offset| body_start + offset)
        .ok_or_else(|| DocsError::UnterminatedBlock(block.to_string()))?;

    let mut synced = String::with_capacity(text.len() + content.len());
    synced.push_str(&text[..start]);
    synced.push_str(&begin);
    synced.push_str(eol);
    synced.push_str(&content.replace('\n', eol));
    synced.push_str(eol);
    synced.push_str(&end);
    synced.push_str(&text[body_end + end.len()..]);
    Ok(synced)
}

fn line_ending(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    if crlf > lf {
        "\r\n"
    } else {
        "\n"
    }
}

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let widths = headers
        .iter()
        .enumerate()
        .map(|(column, header)| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .chain(std::iter::once(header.chars().count()))
                .max()
                .unwrap_or(0)
                .max(3)
        })
        .collect::<Vec<_>>();

    let line = |cells: &[String]| {
        let padded = cells
            .iter()
            .enumerate()
            .map(|(column, cell)| format!("{:<width$}", cell, width = widths[column]))
            .collect::<Vec<_>>()
            .join(" | ");
        format!("| {padded} |")
    };

    let header = line(
        &headers
            .iter()
            .map(|header| header.to_string())
            .collect::<Vec<_>>(),
    );
    let separator = line(
        &widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>(),
    );

    std::iter::once(header)
        .chain(std::iter::once(separator))
        .chain(rows.iter().map(|row| line(row)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn yes_no(value: bool) -> String {
    if value {
        "Yes".to_string()
    } else {
        "No".to_string()
    }
}

fn capabilities_label(capabilities: UploaderCapabilities) -> String {
    if capabilities.accepts_any() {
        return "All".to_string();
    }

    capabilities
        .names()
        .into_iter()
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(", ")
}

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn trigger_label(trigger: RuleTrigger) -> String {
    match trigger {
        RuleTrigger::Always => "Always",
        RuleTrigger::IfOversized => "If oversized",
    }
    .to_string()
}

fn requirements_label(tools: &[Tool]) -> String {
    if tools.is_empty() {
        return "None (pure Rust)".to_string();
    }

    let names = tools
        .iter()
        .map(|tool| tool.binary())
        .collect::<Vec<_>>()
        .join(" + ");
    format!("{names} on `PATH`")
}

#[cfg(test)]
mod tests {
    use super::*;

    const README: &str = "\
Intro

<!-- BEGIN generated: services -->
stale
<!-- END generated: services -->

Middle

<!-- BEGIN generated: strategies -->
stale
<!-- END generated: strategies -->

Outro
";

    #[test]
    fn sync_replaces_every_block() {
        let synced = sync(README).unwrap();

        assert!(!synced.contains("stale"));
        assert!(synced.contains(&services_table()));
        assert!(synced.contains(&strategies_table()));
        assert!(synced.starts_with("Intro"));
        assert!(synced.ends_with("Outro\n"));
    }

    #[test]
    fn sync_is_idempotent() {
        let synced = sync(README).unwrap();
        assert_eq!(sync(&synced).unwrap(), synced);
    }

    #[test]
    fn sync_preserves_crlf() {
        let readme = README.replace('\n', "\r\n");
        let synced = sync(&readme).unwrap();

        assert!(!synced.replace("\r\n", "").contains('\n'));
        assert_eq!(sync(&synced).unwrap(), synced);
    }

    #[test]
    fn missing_block_is_an_error() {
        assert!(matches!(
            sync("nothing here"),
            Err(DocsError::MissingBlock(_))
        ));
    }

    #[test]
    fn unterminated_block_is_an_error() {
        assert!(matches!(
            sync("<!-- BEGIN generated: services -->\n"),
            Err(DocsError::UnterminatedBlock(_))
        ));
    }

    #[test]
    fn optional_token_services_get_a_footnote() {
        let table = services_table();
        let has_optional = UploaderId::ALL
            .iter()
            .any(|id| id.info().credential == Credential::OptionalToken);

        assert_eq!(table.contains(OPTIONAL_TOKEN_NOTE), has_optional);
    }
}
