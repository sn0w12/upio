# upio

Upload files to multiple hosting services from the command line or your own
Rust code, with optional preprocessing (splitting, compression, renaming) so
files fit the target service's limits. A desktop GUI is also included.

Currently supported services:

| Service     | Crate                | Token | Folder / album |
| ----------- | -------------------- | ----- | -------------- |
| Bunkr.cr    | `uploader-bunkr`     | Yes   | Yes            |
| GoFile.io   | `uploader-gofile`    | No\*  | Yes            |
| Fileditch   | `uploader-fileditch` | No    | No             |
| Filester.me | `uploader-filester`  | Yes   | Yes            |

\* Optional. Anonymous uploads work without one, a token raises the size limit and unlocks folder management.

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

The CLI has four subcommands. Running `upio <file>...` with no subcommand
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

| Flag                | Description                                                                                      |
| ------------------- | ------------------------------------------------------------------------------------------------ |
| `-u, --uploaders`   | Services to use. Repeatable; repeated names are deduplicated; defaults to `bunkr`.               |
| `-f, --folder-id`   | Destination folder/album ID.                                                                     |
| `-n, --folder-name` | Folder/album name; resolved by name, creating it if it does not exist (Bunkr, GoFile, Filester). |
| `-b, --batch-size`  | Maximum concurrent uploads; sequential when unset.                                               |
| `-g, --glob`        | Extra glob patterns to match (e.g. `'**/*.mp4'`).                                                |

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

[bunkr]
token = "…"
folder_id = "…"

[gofile]
token = "…"                 # optional; anonymous uploads work without it
folder_id = "…"
server = "store4"           # optional; auto-selected when unset

[fileditch]

[filester]
token = "…"
folder_id = "…"

# Per-service preprocessing lives under e.g. [bunkr.preprocess]; see below.
```

Unknown keys in `disabled_uploaders` are inert; only exact canonical service
names disable anything.

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

| #   | Strategy         | Trigger      | What it does                                                                                                                    | Requirements                   |
| --- | ---------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| 1   | `normalize_name` | Always       | Sanitize the file name (spaces, unicode, invalid characters); length-capped, Unicode-safe. The original file is never modified. | None                           |
| 2   | `compress_image` | If oversized | Re-encode and/or resize images. Output `jpeg` or `png`.                                                                         | None (pure Rust)               |
| 3   | `wrap_zip`       | Always       | Wrap the file in a compressed `.zip` (useful for text/log files). Skips images and videos.                                      | None (pure Rust)               |
| 4   | `split_video`    | If oversized | Split a video into independently playable segments via ffmpeg.                                                                  | `ffmpeg` + `ffprobe` on `PATH` |

When a strategy needs an external tool that is missing, the rule is skipped
rather than failing the upload. Use `upio preprocess check` to see what is
available.

## Library

Use the `upio` crate to upload from your own Rust program. The crate re-exports
every service; feature flags control which ones are compiled in
(`default = ["bunkr", "gofile", "fileditch", "filester"]`).

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

Uploaders declare what they accept via `capabilities()` (video, image, audio,
archives, arbitrary files) and their maximum file size via `max_file_size()`;
both drive preprocessing. The `Uploader` trait also exposes
`get_or_create_folder(folder_name, config)`, which resolves a folder by name
and creates it when it does not exist. The `config` argument supplies per-call
settings such as a token that differs from the one the uploader was built
with; GoFile and Filester fall back from `config.token` to their own stored
credential. Bunkr, GoFile, and Filester implement folder resolution; others
report no folder support.

When you don't want to construct an uploader by hand, use the `upio::registry`
module. It is the single source of truth for known uploaders: `UploaderId`
enumerates them and `registry::build_uploader(id, config)` builds one from an
endpoint config, the same path the CLI uses.

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

| Crate                       | Purpose                                                                         |
| --------------------------- | ------------------------------------------------------------------------------- |
| `crates/core`               | Shared types, traits, error handling, HTTP helpers, and preprocessing.          |
| `crates/uploader-bunkr`     | Bunkr.cr implementation.                                                        |
| `crates/uploader-gofile`    | GoFile.io implementation.                                                       |
| `crates/uploader-fileditch` | Fileditch implementation.                                                       |
| `crates/uploader-filester`  | Filester.me implementation.                                                     |
| `crates/upio`               | Unified library: feature flags, the upload pipeline, and the uploader registry. |
| `crates/config`             | Config model, layered env/file reader, typed keys, and atomic writes.           |
| `crates/cli`                | The `upio` command-line binary.                                                 |
| `crates/gui`                | The `upio-gui` Dioxus desktop app.                                              |

Versioning: `upio-core`, the per-service `uploader-*` crates, and the `upio`
crate are released together from the workspace version. The `upio-config` and
`upio-cli` crates are versioned independently.

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

CI runs the same checks plus `cargo check -p upio --no-default-features`.

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
