# upio

Upload and download files from multiple hosting services in your own Rust code,
with optional preprocessing (splitting, compression, renaming) so files fit the
target service's limits. Uploading is also available from the command line, and
a desktop GUI is included.

Currently supported services:

<!-- BEGIN generated: services -->

| Service     | Crate                 | Auth Required | Folder / album | Download | Capabilities |
| ----------- | --------------------- | ------------- | -------------- | -------- | ------------ |
| Bunkr.cr    | `uploader-bunkr`      | Yes           | Yes            | Yes      | All          |
| GoFile.io   | `uploader-gofile`     | Optional\*    | Yes            | Yes      | All          |
| Fileditch   | `uploader-fileditch`  | No            | No             | Yes      | All          |
| Filester.me | `uploader-filester`   | Optional\*    | Yes            | Yes      | All          |
| GoonBox     | `uploader-goonbox`    | Yes           | Yes            | Yes      | Image        |
| Pixeldrain  | `uploader-pixeldrain` | Yes           | Yes            | Yes      | All          |

\* Optional. Anonymous uploads work without auth, however some functionality may be locked behind it.

<!-- END generated: services -->

Downloads are opt-in. The Download column shows support when the `download`
feature is enabled.

## Quick start

Build and run the CLI from the repo:

```sh
cargo run -p upio-cli -- list
```

Or install it and use `upio` directly:

```sh
cargo install --path crates/cli
upio --help
```

Set a Bunkr token and upload a file:

```sh
upio config set bunkr.token <your-token>
upio upload -u bunkr video.mp4
```

## Command line

The CLI has five subcommands. Running `upio <file>...` with no subcommand
also uploads.

### `upload`

Upload files to one or more services.

```sh
# Upload to a single service (default: bunkr)
upio upload video.mp4

# Upload to several services at once
upio upload -u bunkr -u gofile video.mp4

# Upload every file in a directory (recursively; hidden entries are skipped)
upio upload -u filester ./videos

# Upload files matching a glob pattern
upio upload -u bunkr -g '**/*.mp4'

# Place files in a specific folder/album
upio upload -u bunkr -f 1234ab video.mp4
upio upload -u bunkr -n "My Album" video.mp4   # found or created by name

# Upload up to 4 files concurrently
upio upload -u bunkr -b 4 video-*.mp4
```

| Flag                | Description                                                                                                           |
| ------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `-u, --uploaders`   | Services to use. Repeatable; repeated names are deduplicated; defaults to `bunkr`.                                    |
| `-f, --folder-id`   | Destination folder/album ID.                                                                                          |
| `-n, --folder-name` | Folder/album name; resolved by name, creating it if it does not exist (Bunkr, GoFile, Filester, GoonBox, Pixeldrain). |
| `-b, --batch-size`  | Maximum concurrent uploads; sequential when unset.                                                                    |
| `-g, --glob`        | Extra glob patterns to match (e.g. `'**/*.mp4'`).                                                                     |

Glob patterns support `*`, `?`, `**` across directories, and `[...]` character
classes (single characters, `a-z` ranges, negation with a leading `!` or `^`).
Hidden entries (names starting with `.`) are never matched or walked. Results
print as a table with a colored status and a per-service summary line.

Disabled services are never contacted. An explicit `-u <name>` for a disabled
service reports it as disabled without building an uploader, and if every
requested service is disabled the command fails with "no enabled uploaders
selected" before any network request.

A successful upload that the service answers without a usable URL counts as
failed; the pipeline never reports success with no URL. If preprocessing
created temporary files and cleanup fails after the upload, the URLs are kept
but the row still reports the cleanup error.

### `download`

Download files from a share URL. Credentials come from the matching service
config, so private content works without extra flags.

```sh
upio download 'https://filester.me/f/<id>'
upio download -o downloads 'https://pixeldrain.com/u/<id>'
upio download --dry-run 'https://gofile.io/d/<id>'
```

| Flag                | Description                                                                                |
| ------------------- | ------------------------------------------------------------------------------------------ |
| `-o, --output`      | Output directory. Defaults to `global.download.output_dir`, then the current directory.    |
| `-c, --concurrency` | Maximum concurrent files. Defaults to `global.download.concurrency`.                       |
| `--no-resume`       | Start partial files over instead of resuming them.                                        |
| `--on-conflict`     | `rename`, `skip`, or `overwrite`. Defaults to `global.download.on_conflict`.               |
| `-p, --password`    | Password for protected links without `?password=`.                                         |
| `--flat`            | Put files directly in the output directory instead of preserving remote folder names.      |
| `--dry-run`         | Print the planned destinations without writing files.                                      |

`-p` and `<service>.link_password` supply protected-link passwords; a
`?password=` on the URL wins.

### `config`

Manage the configuration file.

```sh
upio config path                     # print the config file location
upio config get                      # dump the whole config
upio config get bunkr.token          # one value (secrets masked)
upio config get --show-secrets       # dump the whole config unmasked
upio config get --show-secrets bunkr.token
upio config set gofile.token <token>
upio config set gofile.folder_id 123
upio config set global.disabled_uploaders "filester"
```

Setting a value to `none` clears it. Token values are masked as `[REDACTED]`
by default; pass `--show-secrets` to print them in full. Writes are atomic:
the file is replaced whole, so an interrupted write can never leave a
truncated config.

### `list`

Show the available services, whether they are enabled, and what file kinds they
accept:

```sh
upio list
```

See the disabled-service behavior under [`upload`](#upload).

### `preprocess check`

Report which external tools are installed and validate the configured
preprocessing rules:

```sh
upio preprocess check
```

## Configuration

The CLI reads a TOML file at `<config-dir>/upio/config.toml`
(`~/.config/upio/` on Linux, `%APPDATA%\upio\` on Windows,
`~/Library/Application Support/upio/` on macOS). Run `upio config path` for the
exact location. Point at an alternate file with `--config <path>` (available on
every subcommand).

Values can also come from environment variables prefixed with `UPIO_`, with `_`
mapped to `.`, e.g. `UPIO_BUNKR_TOKEN` sets `bunkr.token`. Environment
variables take precedence over the config file.

```toml
[global]
disabled_uploaders = []
disabled_downloaders = []

[global.download]
output_dir = "…"       # optional; defaults to the current directory
concurrency = 4
resume = true
on_conflict = "rename" # rename | skip | overwrite

[bunkr]
token = "…"
folder_id = "…"

[gofile]
token = "…" # optional; anonymous uploads work without it
folder_id = "…"
server = "store4" # optional; auto-selected when unset
link_password = "…" # optional; unlocks password-protected folders

[fileditch]

[filester]
token = "…"
folder_id = "…"
link_password = "…" # optional; unlocks password-protected folders

[goonbox]
token = "…" # optional; read-only API key for link-only downloads
username = "…" # required for uploads
password = "…" # required for uploads; masked in output like a token
folder_id = "…" # an album's encoded_id

[pixeldrain]
token = "…" # required; see the API keys page
folder_id = "…" # a list's id

# Per-service preprocessing lives under e.g. [bunkr.preprocess]; see below.
```

Unknown keys in `disabled_uploaders` or `disabled_downloaders` are inert; only
exact canonical service names disable anything. The two lists are independent,
so a service can be enabled for one direction and disabled for the other.

## Preprocessing

Files can be transformed before upload so they fit the service's limits. Rules
match files by MIME type and **chain**: they always run in a fixed canonical
order (`normalize_name` → `compress_image` → `wrap_zip` → `split_video`),
feeding each output into the next rule regardless of how they are listed.
Preprocessing is configured per service inside the config file:

```toml
[bunkr.preprocess]
enabled = true

[[bunkr.preprocess.rules]]
strategy = "normalize_name"
params = { replacement = "_", lowercase = true }

[[bunkr.preprocess.rules]]
strategy = "compress_image"
params = { format = "jpeg", quality = 80, max_dimensions = "4096x4096" }
```

### Rule fields

A rule has exactly two fields; being listed is what enables it:

| Field      | Description                                  |
| ---------- | -------------------------------------------- |
| `strategy` | The transformation to apply.                 |
| `params`   | Strategy-specific options (all defaultable). |

The strategy owns everything else:

- Which MIME types it applies to. Non-matching files pass through untouched.
- When it runs: on every match, or only when the file exceeds the target
  service's maximum file size. The threshold always comes from the service,
  never from the config.
- Which external tools it needs. A missing tool skips the rule instead of
  failing the upload.

There is no per-rule `trigger`, `match_types`, or other override field. Such
keys in the config are ignored.

Temporary artifacts are removed automatically after upload. If cleanup fails,
the upload's URLs are preserved but the result reports the cleanup error.

### Strategies

Strategies run in this canonical chain order:

<!-- BEGIN generated: strategies -->

| #   | Strategy         | Trigger      | What it does                                                                                                                    | Requirements               |
| --- | ---------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| 1   | `normalize_name` | Always       | Sanitize the file name (spaces, unicode, invalid characters); length-capped, Unicode-safe. The original file is never modified. | None (pure Rust)           |
| 2   | `compress_image` | If oversized | Re-encode and/or resize images; outputs jpeg or png.                                                                            | None (pure Rust)           |
| 3   | `wrap_zip`       | Always       | Wrap the file in a compressed zip (useful for text and log files). Skips images and videos.                                     | None (pure Rust)           |
| 4   | `split_video`    | If oversized | Split a video into independently playable segments via ffmpeg and ffprobe.                                                      | ffmpeg + ffprobe on `PATH` |

<!-- END generated: strategies -->

When a strategy needs an external tool that is missing, the rule is skipped
rather than failing the upload. Use `upio preprocess check` to see what is
available.

## Library

Use the `upio` crate to upload from your own Rust program. The crate re-exports
every service; feature flags control which ones are compiled in
(`default = ["bunkr", "gofile", "fileditch", "filester", "goonbox", "pixeldrain"]`).
Downloads are behind the opt-in `download` feature, which is off by default.

```toml
[dependencies]
upio = { path = "crates/upio" }
anyhow = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use upio::pipeline;
use upio::{BunkrUploader, UploaderEndpointConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let uploader = BunkrUploader::with_token("your-token");
    let config = UploaderEndpointConfig {
        folder_id: Some("1234ab".to_string()),
        ..Default::default()
    };

    // Upload a single file. init() runs first; a "successful" response
    // without a URL comes back as an error, not a silent empty list.
    let result = pipeline::upload_file("video.mp4", &uploader, &config).await;
    println!("{:?} {:?}", result.urls, result.error);

    // Upload a batch, up to 4 at a time.
    let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
    let results = pipeline::upload_files(&files, &uploader, &config, Some(4)).await;
    for r in results {
        println!("{} -> {:?} {:?}", r.file, r.urls, r.error);
    }

    Ok(())
}
```

Every service implements the `Uploader` trait, so the same pipeline works for
any of them. The pipeline calls `init()` before every upload, plain and
preprocessed alike, so token verification happens exactly once per file and a
failed init performs zero upload calls.

Constructors are synchronous. Network setup happens in `init()`:

- `BunkrUploader::with_token(token)`, requires a token.
- `GoFileUploader::new()` or `GoFileUploader::with_token(token)`, token optional.
- `FileditchUploader::new()`, no token.
- `FilesterUploader::new()` or `FilesterUploader::with_token(token)`, token optional.
- `PixeldrainUploader::new()` or `PixeldrainUploader::with_token(token)`,
  token required; Pixeldrain rejects anonymous uploads.
- `GoonboxUploader::new(username, password)`. Uploads use an account, not a
  token: GoonBox has no browser-free write API, so `init()` signs in and
  solves the site's Cloudflare challenge with a real Chrome. On a headless
  Linux server, install Xvfb (`apt install xvfb`); upio starts it
  automatically when `DISPLAY` is unset.

Uploaders declare what they accept via `capabilities()` (video, image, audio,
archives, arbitrary files) and their maximum file size via `max_file_size()`;
both drive preprocessing. The `Uploader` trait also exposes
`get_or_create_folder(folder_name, config)`, which resolves a folder by name
and creates it when it does not exist. The `config` argument supplies per-call
settings such as a token that differs from the one the uploader was built
with; GoFile and Filester fall back from `config.token` to their own stored
credential. Bunkr, GoFile, Filester, GoonBox, and Pixeldrain implement folder
resolution; others report no folder support.

When you don't want to construct an uploader by hand, use the `upio::registry`
module. It is the single source of truth for known uploaders: `UploaderId`
enumerates them and `registry::build_uploader(id, config)` builds one from an
endpoint config, the same path the CLI uses.

Downloads use the `Downloader` trait. A service implements `resolve` to turn a
share URL into a tree of files, and the shared engine handles naming,
conflicts, resume, retries, and atomic completion. Enable `download` on the
dependency to compile it:

```rust
use upio::download::{self, DownloadOptions, DirectDownloader, Downloader};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let downloader = DirectDownloader::new();
    let node = downloader
        .resolve("https://cdn.example.com/clip.mp4", &Default::default())
        .await?;
    let plan = download::plan(
        &node,
        &DownloadOptions {
            output_dir: "videos".into(),
            ..Default::default()
        },
    )?;

    let results = download::download(&plan, &downloader, &Default::default(), 4, None).await;
    for result in results {
        println!("{} -> {:?} {:?}", result.source_url, result.destination, result.error);
    }
    Ok(())
}
```

A service advertises download support by adding a `DownloadDef` to its
`ServiceDef`. `UploaderId::from_url` dispatches a URL to the service that claims
it, and `registry::build_downloader(id, config)` builds the downloader. A
service without a descriptor is upload-only.

Downloaders use the same `token` and account fields as uploaders.
`GoonboxDownloader` also accepts an account with `with_account`; GoFile and
Filester read protected-link passwords from `link_password` or `?password=`.

Fileditch share pages gate their direct link behind a WebAssembly browser check.
Its downloader runs a real Chrome to pass that check, reads the link the page
assigns to its download button, and hands the public URL to the shared engine.
Chrome must be installed. GoonBox and Fileditch share one profile at
`<config-dir>/upio/chrome-profile`, so clearance cookies are paid for once.
Chrome runs without a visible window: on Windows it starts on a separate
desktop, and on headless Linux under Xvfb when it is available.

## GUI

The Dioxus 0.7 desktop app is in `crates/gui` (`upio-gui`). It shares
`config.toml` with the CLI and keeps its own preferences (theme, window size,
remembered services and concurrency) in `gui.toml` next to it. Both files are
written atomically. A malformed file shows an in-app error banner instead of
being silently overwritten with defaults.

```sh
cargo install dioxus-cli   # once
dx serve --platform desktop
```

The queue supports drag-and-drop with an explicit confirm step before anything
uploads, per-file per-service progress and outcomes, and a "Stop after current
upload" button. Disabled services are filtered out of the picker, and all
controls are reachable by keyboard.

## Workspace layout

| Crate                     | Purpose                                                                         |
| ------------------------- | ------------------------------------------------------------------------------- |
| `crates/core`             | Shared types, traits, errors, HTTP helpers, the download engine, and preprocessing. |
| `crates/browser`          | Shared Chrome session, hidden desktop/Xvfb, and the one Chrome profile.         |
| `crates/uploader-*bunkr*` | Uploader implementations.                                                       |
| `crates/upio`             | Unified library: feature flags, the upload pipeline, and the service registry.  |
| `crates/config`           | Config model, layered env/file reader, typed keys, and atomic writes.           |
| `crates/cli`              | The `upio` command-line binary.                                                 |
| `crates/gui`              | The `upio-gui` Dioxus desktop app.                                              |

Versioning: `upio-core`, `upio-browser`, the per-service `uploader-*` crates, and
the `upio` crate are released together from the workspace version. The
`upio-config` and `upio-cli` crates are versioned independently.

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

CI runs the same checks plus `cargo check -p upio --no-default-features`.

The services and strategies tables above are generated from the code. After
adding an uploader or preprocessing strategy, regenerate them:

```sh
cargo run -p upio --example readme --all-features
```

`cargo test -p upio --all-features` fails when the README and the code disagree.
The generated table assumes every feature, including `download`, is enabled.

Build an optimized binary (release profile is configured in the workspace root):

```sh
cargo build --release -p upio-cli
```

## License

Licensed under either of the following, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work, as defined in the Apache-2.0 license,
is licensed under both, without additional terms or conditions.
