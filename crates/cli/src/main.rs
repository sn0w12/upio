mod cli;
mod commands;
mod input;
mod live;
mod output;
mod style;
mod upload;

use clap::{CommandFactory, Parser};

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    style::init();
    let cli = Cli::parse();
    let config_path = cli.config.as_deref();

    match cli.command {
        Some(Commands::Upload { args }) => commands::upload::run(args, config_path).await?,
        Some(Commands::Download { args }) => commands::download::run(args, config_path).await?,
        Some(Commands::Mirror { args }) => commands::mirror::run(args, config_path).await?,
        Some(Commands::Config { action }) => commands::config::run(action, config_path)?,
        Some(Commands::List) => commands::list::run(config_path)?,
        Some(Commands::Preprocess {
            action: cli::PreprocessAction::Check,
        }) => {
            commands::preprocess::run(config_path)?;
        }
        #[cfg(debug_assertions)]
        Some(Commands::Demo { action }) => commands::demo::run(action).await?,
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
