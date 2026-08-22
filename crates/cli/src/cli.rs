use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "upio",
    version,
    about = "Upload files to various hosting services"
)]
pub struct Cli {
    /// Path to an alternate config file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub upload: UploadArgs,
}

/// Arguments shared by the top-level command and the `upload` subcommand.
#[derive(Debug, Parser)]
#[command(about = "Upload files to various hosting services")]
pub struct UploadArgs {
    /// Paths to files or directories to upload
    pub paths: Vec<String>,

    /// Additional glob patterns, e.g. '**/*.mp4'
    #[arg(short = 'g', long)]
    pub glob: Vec<String>,

    /// Uploader(s) to use. Can be specified multiple times: -u bunkr -u gofile
    #[arg(short = 'u', long, default_values_t = default_uploaders())]
    pub uploaders: Vec<String>,

    /// Folder/album ID
    #[arg(short = 'f', long)]
    pub folder_id: Option<String>,

    /// Folder/album name (looked up by name)
    #[arg(short = 'n', long)]
    pub folder_name: Option<String>,

    /// Maximum concurrent uploads (default: sequential)
    #[arg(short = 'b', long)]
    pub batch_size: Option<usize>,
}

fn default_uploaders() -> Vec<String> {
    vec!["bunkr".to_string()]
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Upload files
    Upload {
        #[command(flatten)]
        args: UploadArgs,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// List available uploaders
    List,
    /// Inspect the preprocessing setup
    Preprocess {
        #[command(subcommand)]
        action: PreprocessAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Get a config value or all values
    Get {
        /// Optional key to get
        key: Option<String>,
        /// Show secret values (tokens) in full instead of masking them
        #[arg(long)]
        show_secrets: bool,
    },
    Set {
        /// Config key (e.g., bunkr.token, global.disabled_uploaders)
        key: String,
        /// Value
        value: String,
    },
    /// Show the config file path
    Path,
}

#[derive(Debug, Subcommand)]
pub enum PreprocessAction {
    /// Check which external tools are available and validate configured rules
    Check,
}
