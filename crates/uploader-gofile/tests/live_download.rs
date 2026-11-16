#![cfg(feature = "download")]

use upio_core::download::{Downloader, RemoteNode};
use uploader_gofile::GoFileDownloader;

#[tokio::test]
#[ignore = "requires a live GoFile share URL in UPIO_GOFILE_URL"]
async fn resolves_a_live_share_url() {
    let url = std::env::var("UPIO_GOFILE_URL").expect("set UPIO_GOFILE_URL");
    let downloader = GoFileDownloader::new();
    let node = downloader
        .resolve(&url, &Default::default())
        .await
        .expect("resolution should succeed");

    match &node {
        RemoteNode::File(file) => assert!(!file.name.is_empty()),
        RemoteNode::Folder { name, children } => {
            assert!(!name.is_empty());
            assert!(!children.is_empty());
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
