mod cli;
mod commands;
mod input;
mod output;
mod upload;

use clap::{CommandFactory, Parser};

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.as_deref();

    match cli.command {
        Some(Commands::Upload { args }) => commands::upload::run(args, config_path).await?,
        Some(Commands::Config { action }) => commands::config::run(action, config_path)?,
        Some(Commands::List) => commands::list::run(config_path)?,
        Some(Commands::Preprocess {
            action: cli::PreprocessAction::Check,
        }) => {
            commands::preprocess::run(config_path)?;
        }
        None => {
            if cli.upload.paths.is_empty() {
                Cli::command().print_help()?;
                return Ok(());
            }
            commands::upload::run(cli.upload, config_path).await?;
        }
    }

    Ok(())
}
