use upio_core::download::{Downloader, RemoteNode};
use uploader_fileditch::FileditchDownloader;

#[tokio::test]
#[ignore = "requires Chrome and a live share URL in UPIO_FILEDITCH_URL"]
async fn resolves_a_live_share_url() {
    let url = std::env::var("UPIO_FILEDITCH_URL").expect("set UPIO_FILEDITCH_URL");
    let downloader = FileditchDownloader::new();
    let node = downloader
        .resolve(&url, &Default::default())
        .await
        .expect("resolution should succeed");

    match node {
        RemoteNode::File(file) => {
            assert!(!file.name.is_empty());
            assert!(file.url.starts_with("http"));
        }
        RemoteNode::Folder { .. } => panic!("a share URL should resolve to one file"),
    }
}
