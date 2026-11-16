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

    report("global", &config.preprocess);
    for id in UploaderId::ALL {
        if let Some(preprocess) = config.preprocess_override(id.name()) {
            report(id.name(), preprocess);
        }
    }

    Ok(())
}

fn report(label: &str, preprocess: &upio::preprocess::PreprocessConfig) {
    if !preprocess.enabled {
        return;
    }
    match upio::preprocess::validate_config(preprocess) {
        Ok(()) => println!("{}: preprocess rules valid", label),
        Err(e) => println!("{}: invalid preprocess config: {}", label, e),
    }
}
