#![cfg(feature = "download")]

use upio_core::download::{Downloader, RemoteNode};
use uploader_bunkr::BunkrDownloader;

#[tokio::test]
#[ignore = "requires a live Bunkr file or album URL in UPIO_BUNKR_URL"]
async fn resolves_a_live_share_url() {
    let url = std::env::var("UPIO_BUNKR_URL").expect("set UPIO_BUNKR_URL");
    let downloader = match std::env::var("UPIO_BUNKR_TOKEN") {
        Ok(token) => BunkrDownloader::with_token(&token),
        Err(_) => BunkrDownloader::new(),
    };
    let node = downloader
        .resolve(&url, &Default::default())
        .await
        .expect("resolution should succeed");

    match &node {
        RemoteNode::File(file) => {
            assert!(!file.name.is_empty());
            assert!(file.url.contains("upio-id="));
        }
        RemoteNode::Folder { name, children } => {
            assert!(!name.is_empty());
            assert!(!children.is_empty());
            assert!(children.iter().all(|child| match child {
                RemoteNode::File(file) => !file.name.is_empty(),
                RemoteNode::Folder { .. } => false,
            }));
        }
    }

    let first_file = match &node {
        RemoteNode::File(file) => Some(file),
        RemoteNode::Folder { children, .. } => children.iter().find_map(|child| match child {
            RemoteNode::File(file) => Some(file),
            RemoteNode::Folder { .. } => None,
        }),
    };
    if let Some(file) = first_file {
        let mut response = downloader
            .open(file, 0, &Default::default())
            .await
            .expect("the direct link should open");
        assert!(response.status().is_success());
        let chunk = response
            .chunk()
            .await
            .expect("reading the first chunk should work")
            .expect("the file should not be empty");
        assert!(!chunk.is_empty());
    }
}
