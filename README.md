# upio

Upload and download files across multiple hosting services from the command
line or the desktop app. Preprocessing can normalize, compress, zip, or split
files so they fit each service's limits.

Currently supported services:

<!-- BEGIN generated: services -->

| Service     | Auth Required | Folder / album | Download | Capabilities |
| ----------- | ------------- | -------------- | -------- | ------------ |
| Bunkr.cr    | Yes           | Yes            | Yes      | All          |
| GoFile.io   | Optional\*    | Yes            | Yes      | All          |
| Fileditch   | Optional\*    | No             | Yes      | All          |
| Filester.me | Optional\*    | Yes            | Yes      | All          |
| GoonBox     | Yes           | Yes            | Yes      | Image        |
| Pixeldrain  | Yes           | Yes            | Yes      | All          |

\* Optional. Anonymous uploads work without auth, however some functionality may be locked behind it.

<!-- END generated: services -->

## Install

Every release ships a prebuilt CLI and desktop installers. Install the CLI with
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```sh
cargo binstall --git https://github.com/sn0w12/upio upio-cli
```

Or build it from the repository:

```sh
cargo install --path crates/cli
cargo run -p upio-cli -- list
```

The desktop installers are attached to each release.

## Quick start

Set a Bunkr token and upload a file:

```sh
upio config set bunkr.token <your-token>
upio upload -u bunkr video.mp4
```

## Command line

The CLI has six subcommands. Running `upio <file>...` with no subcommand also
uploads.

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

Glob patterns support `*`, `?`, `**`, and `[...]` classes; hidden entries are
skipped. Disabled services are skipped, and if every requested service is
disabled the command fails before any network request.

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

### `mirror`

Download one or more shares, then upload the files to one or more services:

```sh
upio mirror 'https://filester.me/f/<id>' 'https://filester.me/f/<id2>' -u bunkr -u gofile
```

`-n`/`-f` choose a destination folder or album, `-b` sets upload concurrency,
`-c` caps concurrent downloads across every share, and `-p` unlocks a protected
source. Downloads go to a temporary directory; `--staging <dir>` reuses a
directory and `--keep` downloads to `global.download.output_dir` (or the current
directory) and leaves the files there.

### `config`

Manage the configuration file.

```sh
upio config path                     # print the config file location
upio config get                      # grouped summary of every key
upio config get bunkr.token          # one value (secrets masked)
upio config get --show-secrets       # grouped summary with secrets in full
upio config set gofile.token <token>
upio config set gofile.folder_id 123
upio config set global.disabled_uploaders "filester"
```

`config get` prints the grouped summary when stdout is a terminal and raw TOML
otherwise, so scripts and pipes keep working. The summary also shows the global
preprocessing rules and any per-service overrides. Setting a value to `none`
clears it. Secret values are masked by default, including in the TOML output;
pass `--show-secrets` to print them in full. Writes are atomic.

### `list`

Show the available services, whether they are enabled, and what file kinds they
accept:

```sh
upio list
```

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
output_dir = "."       # optional; defaults to the current directory
concurrency = 4
resume = true
on_conflict = "rename" # rename | skip | overwrite

# Default preprocessing for every service. A [service.preprocess] table
# replaces this configuration for that service.
[preprocess]
enabled = false

# [[preprocess.rules]]
# strategy = "normalize_name"

[bunkr]
token = "."
folder_id = "."

[gofile]
token = "." # optional; anonymous uploads work without it
folder_id = "."
server = "store4" # optional; auto-selected when unset
link_password = "." # optional; unlocks password-protected folders

[fileditch]
token = "." # optional; uploads with it are listed in your account

[filester]
token = "."
folder_id = "."
link_password = "." # optional; unlocks password-protected folders

[goonbox]
token = "." # optional; read-only API key for link-only downloads
username = "." # required for uploads
password = "." # required for uploads; masked in output like a token
folder_id = "." # an album's encoded_id

[pixeldrain]
token = "." # required; see the API keys page
folder_id = "." # a list's id

# Per-service preprocessing; see docs/preprocessing.md.
```

Unknown keys in `disabled_uploaders` or `disabled_downloaders` are inert; only
exact canonical service names disable anything. The two lists are independent,
so a service can be enabled for one direction and disabled for the other.

## Preprocessing

Rules are configured globally under `[preprocess]` and can normalize names,
compress images, wrap files in zip, or split videos. Add a
`[service.preprocess]` table to replace the global configuration for one
service. See [docs/preprocessing.md](docs/preprocessing.md) for the rule format,
strategy table, and tool requirements.

## GUI

The desktop app is in `crates/gui` (`upio-gui`), built with
[Rune](https://github.com/anomalyco/rune) and `rune-kit`. It shares
`config.toml` with the CLI and keeps its own preferences (theme, selected
services, concurrency) in `gui.toml` next to it.

```sh
cargo run -p upio-gui
```

Settings edits stay in a draft until you press **Save changes**; **Discard**
restores the last saved config. The queue supports drag-and-drop with a confirm
step, per-file per-service progress and outcomes, and a stop that cancels
uploads in flight.

## Documentation

- [Library usage](docs/library.md)
- [Preprocessing rules](docs/preprocessing.md)
- [Development](docs/development.md)

## License

Licensed under either of the following, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work, as defined in the Apache-2.0 license,
is licensed under both, without additional terms or conditions.
