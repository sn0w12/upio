# Library

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
