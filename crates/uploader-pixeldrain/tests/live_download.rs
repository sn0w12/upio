#![cfg(feature = "download")]

use upio_core::download::{Downloader, RemoteNode};
use uploader_pixeldrain::PixeldrainDownloader;

#[tokio::test]
#[ignore = "requires a live Pixeldrain share URL in UPIO_PIXELDRAIN_URL"]
async fn resolves_a_live_share_url_and_resumes() {
    let url = std::env::var("UPIO_PIXELDRAIN_URL").expect("set UPIO_PIXELDRAIN_URL");
    let downloader = PixeldrainDownloader::new();
    let node = downloader
        .resolve(&url, &Default::default())
        .await
        .expect("resolution should succeed");

    let file = match &node {
        RemoteNode::File(file) => file,
        RemoteNode::Folder { children, .. } => children
            .iter()
            .find_map(|child| match child {
                RemoteNode::File(file) => Some(file),
                RemoteNode::Folder { .. } => None,
            })
            .expect("a Pixeldrain list should contain files"),
    };
    assert!(!file.name.is_empty());

    let mut response = downloader
        .open(file, 1, &Default::default())
        .await
        .expect("a resumed transfer should open");
    assert!(response.status().is_success());
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let size = file.size.expect("file info should carry a size");
        assert_eq!(response.content_length(), Some(size - 1));
    }
    let chunk = response
        .chunk()
        .await
        .expect("reading the first chunk should work")
        .expect("the file should not be empty");
    assert!(!chunk.is_empty());
}
