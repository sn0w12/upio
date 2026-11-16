use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "upio",
    version,
    about = "Upload and download files from various hosting services"
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

/// Arguments for the `download` subcommand.
#[derive(Debug, Parser)]
#[command(about = "Download files from supported share URLs")]
pub struct DownloadArgs {
    /// Share URLs to download
    pub urls: Vec<String>,

    /// Output directory (defaults to global.download.output_dir or the current directory)
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Maximum concurrent file downloads (defaults to global.download.concurrency)
    #[arg(short = 'c', long)]
    pub concurrency: Option<usize>,

    /// Do not resume partial files
    #[arg(long)]
    pub no_resume: bool,

    /// What to do when the destination exists
    #[arg(long, value_name = "POLICY", value_parser = ["rename", "skip", "overwrite"])]
    pub on_conflict: Option<String>,

    /// Password for protected share links without a ?password= parameter
    #[arg(short = 'p', long)]
    pub password: Option<String>,

    /// Put files directly in the output directory without remote folder names
    #[arg(long)]
    pub flat: bool,

    /// Resolve and print planned files without downloading
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments for the `mirror` subcommand.
#[derive(Debug, Parser)]
#[command(about = "Download a share and upload it to other services")]
pub struct MirrorArgs {
    /// Share URL to download
    pub url: String,

    /// Destination uploader(s). Repeatable: -u bunkr -u gofile
    #[arg(short = 'u', long, required = true)]
    pub uploaders: Vec<String>,

    /// Destination folder/album ID
    #[arg(short = 'f', long)]
    pub folder_id: Option<String>,

    /// Destination folder/album name (looked up or created)
    #[arg(short = 'n', long)]
    pub folder_name: Option<String>,

    /// Maximum concurrent uploads
    #[arg(short = 'b', long)]
    pub batch_size: Option<usize>,

    /// Maximum concurrent downloads (defaults to global.download.concurrency)
    #[arg(short = 'c', long)]
    pub concurrency: Option<usize>,

    /// Do not resume partial downloads
    #[arg(long)]
    pub no_resume: bool,

    /// Password for a protected source link without ?password=
    #[arg(short = 'p', long)]
    pub password: Option<String>,

    /// Reuse this download directory instead of a temporary one
    #[arg(long)]
    pub staging: Option<PathBuf>,

    /// Keep the temporary download directory after the upload
    #[arg(long)]
    pub keep: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Upload files
    Upload {
        #[command(flatten)]
        args: UploadArgs,
    },
    /// Download files from supported share URLs
    Download {
        #[command(flatten)]
        args: DownloadArgs,
    },
    /// Download a share and upload it to other services
    Mirror {
        #[command(flatten)]
        args: MirrorArgs,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// List available services
    List,
    /// Inspect the preprocessing setup
    Preprocess {
        #[command(subcommand)]
        action: PreprocessAction,
    },
    /// Play simulated transfers without touching the network (debug builds)
    #[cfg(debug_assertions)]
    Demo {
        #[command(subcommand)]
        action: DemoAction,
    },
}

#[cfg(debug_assertions)]
#[derive(Debug, Subcommand)]
pub enum DemoAction {
    /// Play a simulated upload
    Upload {
        /// How many files to show
        #[arg(long, default_value_t = 5)]
        files: usize,
    },
    /// Play a simulated download
    Download {
        /// How many files to show
        #[arg(long, default_value_t = 5)]
        files: usize,
    },
    /// Play a simulated mirror: download a share, then upload its files
    Mirror {
        /// How many files to show
        #[arg(long, default_value_t = 4)]
        files: usize,
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
