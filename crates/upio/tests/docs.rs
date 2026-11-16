#![cfg(all(
    feature = "bunkr",
    feature = "gofile",
    feature = "fileditch",
    feature = "filester",
    feature = "goonbox",
    feature = "pixeldrain",
    feature = "download"
))]

use std::path::{Path, PathBuf};

#[test]
fn generated_tables_match_code() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    assert_synced(
        &root.join("README.md"),
        &["services"],
        "README services table",
    );
    assert_synced(
        &root.join("docs/preprocessing.md"),
        &["strategies"],
        "preprocessing strategy table",
    );
}

fn assert_synced(path: &Path, blocks: &[&str], label: &str) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let expected = upio::docs::sync_blocks(&text, blocks)
        .unwrap_or_else(|error| panic!("{label} is missing generated markers: {error}"));

    assert!(
        text == expected,
        "{label} is out of date; run `cargo run -p upio --example readme --all-features`"
    );
}
