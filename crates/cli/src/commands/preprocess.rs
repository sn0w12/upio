use std::path::Path;

use anyhow::Result;

use upio::registry::UploaderId;
use upio_config::Settings;

use crate::output;

/// Run the `preprocess check` command: report tool availability and validate
/// the configured rules.
pub fn run(config_path: Option<&Path>) -> Result<()> {
    let settings = Settings::load_from_or_default(config_path)?;
    let config = settings.extract()?;

    let tools = upio::preprocess::tools::available_tools()
        .into_iter()
        .map(|(tool, available)| {
            let status = if available { "available" } else { "missing" };
            (tool.binary(), status, String::new())
        })
        .collect::<Vec<_>>();
    output::print_table(&tools);

    for id in UploaderId::ALL {
        let endpoint = config.get_uploader_config(id.name());
        if !endpoint.preprocess.enabled {
            continue;
        }
        match upio::preprocess::validate_config(&endpoint.preprocess) {
            Ok(()) => println!("{}: preprocess rules valid", id.name()),
            Err(e) => println!("{}: invalid preprocess config: {}", id.name(), e),
        }
    }

    Ok(())
}
