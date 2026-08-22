use futures::StreamExt;
use upio_core::http::{counted_file_stream, UploadProgress};

#[tokio::test]
async fn counted_stream_emits_exact_bytes_and_cumulative_progress() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    // 40 KiB spans several 16 KiB chunks.
    let payload: Vec<u8> = (0..40 * 1024).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &payload).unwrap();

    let file = tokio::fs::File::open(&path).await.unwrap();
    let total = payload.len() as u64;

    let seen = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
    let progress_seen = seen.clone();
    let progress = UploadProgress::new(move |uploaded, total| {
        progress_seen.lock().push((uploaded, total));
    });

    let mut stream = Box::pin(counted_file_stream(file, total, Some(progress)));
    let mut bytes = Vec::new();
    while let Some(chunk) = StreamExt::next(&mut stream).await {
        bytes.extend_from_slice(&chunk.unwrap());
    }

    assert_eq!(bytes, payload, "stream must emit the exact file bytes");

    let progress = seen.lock();
    assert!(!progress.is_empty(), "progress callbacks must fire");
    assert_eq!(
        progress.last().copied(),
        Some((total, total)),
        "final progress must report completion"
    );
    for pair in progress.windows(2) {
        assert!(
            pair[1].0 >= pair[0].0,
            "cumulative progress must never go backwards: {pair:?}"
        );
    }
}
