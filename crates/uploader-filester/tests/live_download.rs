#![cfg(feature = "download")]

use upio_core::download::{Downloader, RemoteNode};
use uploader_filester::FilesterDownloader;

#[tokio::test]
#[ignore = "requires a live Filester share URL in UPIO_FILESTER_URL"]
async fn resolves_a_live_share_url() {
    let url = std::env::var("UPIO_FILESTER_URL").expect("set UPIO_FILESTER_URL");
    let downloader = FilesterDownloader::new();
    let node = downloader
        .resolve(&url, &Default::default())
        .await
        .expect("resolution should succeed");

    let RemoteNode::File(file) = node else {
        panic!("a Filester share URL should resolve to one file");
    };
    assert!(!file.name.is_empty());

    let mut response = downloader
        .open(&file, 0, &Default::default())
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
