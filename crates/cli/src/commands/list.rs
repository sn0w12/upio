use std::path::Path;

use anyhow::Result;

use upio::registry::UploaderId;
use upio_config::Settings;

use crate::output;

/// Run the `list` command: show uploaders and what they accept.
pub fn run(config_path: Option<&Path>) -> Result<()> {
    let settings = Settings::load_from_or_default(config_path)?;
    let config = settings.extract()?;
    let disabled = config.global.disabled_uploaders;

    let rows = UploaderId::ALL
        .iter()
        .map(|id| {
            let enabled = !disabled.contains(&id.name().to_string());
            let status = if enabled { "enabled" } else { "disabled" };
            (*id, status, id.capabilities().names().join(", "))
        })
        .collect::<Vec<_>>();

    let rows = rows
        .iter()
        .map(|(id, status, detail)| (id.name(), *status, detail.clone()))
        .collect::<Vec<_>>();

    output::print_table(&rows);
    Ok(())
}
