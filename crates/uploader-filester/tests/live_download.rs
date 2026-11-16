#![cfg(feature = "download")]

use upio_core::download::{Downloader, RemoteNode};
use upio_core::UploaderEndpointConfig;
use uploader_filester::FilesterDownloader;

#[tokio::test]
#[ignore = "requires a live Filester share URL in UPIO_FILESTER_URL (and UPIO_FILESTER_LINK_PASSWORD for protected folders)"]
async fn resolves_a_live_share_url() {
    let url = std::env::var("UPIO_FILESTER_URL").expect("set UPIO_FILESTER_URL");
    let config = UploaderEndpointConfig {
        link_password: std::env::var("UPIO_FILESTER_LINK_PASSWORD").ok(),
        ..Default::default()
    };
    let downloader = FilesterDownloader::new();
    let node = downloader
        .resolve(&url, &config)
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
            .expect("a Filester folder should contain files"),
    };
    assert!(!file.name.is_empty());

    let offset = if file.size.is_some() { 1 } else { 0 };
    let mut response = downloader
        .open(file, offset, &config)
        .await
        .expect("the direct link should open");
    assert!(response.status().is_success());
    if offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let size = file.size.expect("a folder listing should carry a size");
        assert_eq!(response.content_length(), Some(size - 1));
    }
    let chunk = response
        .chunk()
        .await
        .expect("reading the first chunk should work")
        .expect("the file should not be empty");
    assert!(!chunk.is_empty());
}
