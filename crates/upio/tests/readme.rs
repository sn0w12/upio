#![cfg(all(
    feature = "bunkr",
    feature = "gofile",
    feature = "fileditch",
    feature = "filester",
    feature = "goonbox",
    feature = "pixeldrain"
))]

use std::path::PathBuf;

#[test]
fn readme_tables_match_code() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    let readme = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let expected = upio::docs::sync(&readme).expect("README is missing generated markers");

    assert!(
        readme == expected,
        "README tables are out of date; run `cargo run -p upio --example readme`"
    );
}
