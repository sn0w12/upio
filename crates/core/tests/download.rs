use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use upio_core::download::{
    self, ConflictPolicy, DirectDownloader, DownloadOptions, Downloader, RemoteFile,
};

struct TestServer {
    base: String,
    last_range: Arc<Mutex<Option<String>>>,
}

impl TestServer {
    fn start(payload: Vec<u8>, ignore_range: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let last_range = Arc::new(Mutex::new(None));
        let recorded = last_range.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let payload = payload.clone();
                let recorded = recorded.clone();

                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    continue;
                }
                let is_head = request_line.starts_with("HEAD ");
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed
                        .get(..6)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("range:"))
                    {
                        let value = trimmed[6..].trim();
                        range = parse_range(value);
                        *recorded.lock().unwrap() = Some(value.to_string());
                    }
                }

                let (status, body, headers) = match range {
                    Some(start) if !ignore_range && start < payload.len() => {
                        let end = payload.len() - 1;
                        (
                            "206 Partial Content",
                            &payload[start..],
                            format!(
                                "Content-Range: bytes {}-{}/{}\r\n",
                                start,
                                end,
                                payload.len()
                            ),
                        )
                    }
                    _ => ("200 OK", &payload[..], String::new()),
                };

                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\
                     Content-Disposition: attachment; filename=\"clip.bin\"\r\n{headers}\
                     Connection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                if !is_head {
                    stream.write_all(body).unwrap();
                }
            }
        });

        Self {
            base: format!("http://{address}"),
            last_range,
        }
    }

    fn url(&self, name: &str) -> String {
        format!("{}/{name}", self.base)
    }

    fn last_range(&self) -> Option<String> {
        self.last_range.lock().unwrap().clone()
    }
}

fn parse_range(value: &str) -> Option<usize> {
    let value = value.strip_prefix("bytes=")?;
    let (start, _) = value.split_once('-')?;
    start.parse().ok()
}

fn payload() -> Vec<u8> {
    (0..100_000u32).map(|i| (i % 251) as u8).collect()
}

#[tokio::test]
async fn a_direct_url_downloads_to_the_reported_name() {
    let server = TestServer::start(payload(), false);
    let dir = tempfile::tempdir().unwrap();
    let downloader = DirectDownloader::new();

    let node = downloader
        .resolve(&server.url("file"), &Default::default())
        .await
        .unwrap();
    let plan = download::plan(
        &node,
        &DownloadOptions {
            output_dir: dir.path().to_path_buf(),
            ..DownloadOptions::default()
        },
    )
    .unwrap();

    assert_eq!(plan.files[0].destination, dir.path().join("clip.bin"));

    let results = download::download(&plan, &downloader, &Default::default(), 1, None).await;
    assert!(results[0].error.is_none(), "{:?}", results[0].error);
    assert_eq!(
        std::fs::read(dir.path().join("clip.bin")).unwrap(),
        payload()
    );
    assert!(!dir.path().join("clip.bin.part").exists());
}

#[tokio::test]
async fn a_partial_file_resumes_from_its_current_size() {
    let server = TestServer::start(payload(), false);
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("clip.bin");
    let partial = dir.path().join("clip.bin.part");
    std::fs::write(&partial, &payload()[..50_000]).unwrap();

    let file = RemoteFile::new("clip.bin", server.url("file"));
    let bytes = download::download_file(
        &file,
        &destination,
        &DirectDownloader::new(),
        &Default::default(),
        true,
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, payload().len() as u64);
    assert_eq!(server.last_range().as_deref(), Some("bytes=50000-"));
    assert_eq!(std::fs::read(&destination).unwrap(), payload());
    assert!(!partial.exists());
}

#[tokio::test]
async fn a_server_that_ignores_range_restarts_the_transfer() {
    let server = TestServer::start(payload(), true);
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("clip.bin");
    std::fs::write(dir.path().join("clip.bin.part"), b"stale").unwrap();

    let file = RemoteFile::new("clip.bin", server.url("file"));
    download::download_file(
        &file,
        &destination,
        &DirectDownloader::new(),
        &Default::default(),
        true,
        None,
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), payload());
}

#[tokio::test]
async fn skip_policy_leaves_existing_files_untouched() {
    let server = TestServer::start(payload(), false);
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("clip.bin");
    std::fs::write(&existing, b"keep me").unwrap();

    let node = downloader_resolved(&server).await;
    let plan = download::plan(
        &node,
        &DownloadOptions {
            output_dir: dir.path().to_path_buf(),
            on_conflict: ConflictPolicy::Skip,
            ..DownloadOptions::default()
        },
    )
    .unwrap();

    let results = download::download(
        &plan,
        &DirectDownloader::new(),
        &Default::default(),
        1,
        None,
    )
    .await;
    assert!(results[0].skipped);
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep me");
    assert!(server.last_range().is_none(), "no request should be made");
}

async fn downloader_resolved(server: &TestServer) -> upio_core::download::RemoteNode {
    DirectDownloader::new()
        .resolve(&server.url("file"), &Default::default())
        .await
        .unwrap()
}
