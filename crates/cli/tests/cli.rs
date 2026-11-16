use assert_cmd::Command;

#[test]
fn help_smoke() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.arg("--help").assert().success();
}

#[test]
fn version_flag() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.arg("--version").assert().success();
}

#[test]
fn upload_help() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["upload", "--help"]).assert().success();
}

#[test]
fn upload_without_paths_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.arg("upload").assert().failure();
}

#[test]
fn download_help() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["download", "--help"]).assert().success();
}

#[test]
fn download_without_urls_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.arg("download").assert().failure();
}

#[test]
fn download_unknown_url_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["download", "https://example.test/file"])
        .assert()
        .failure();
}

#[test]
fn download_zero_concurrency_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["download", "-c", "0", "https://example.test/file"])
        .assert()
        .failure();
}

#[test]
fn mirror_help() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["mirror", "--help"]).assert().success();
}

#[test]
fn mirror_without_url_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["mirror", "-u", "bunkr"]).assert().failure();
}

#[test]
fn mirror_without_uploaders_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["mirror", "https://example.test/file"])
        .assert()
        .failure();
}

#[test]
fn mirror_unknown_url_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["mirror", "https://example.test/file", "-u", "bunkr"])
        .assert()
        .failure();
}

#[test]
fn mirror_zero_concurrency_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args([
        "mirror",
        "https://example.test/file",
        "-u",
        "bunkr",
        "-c",
        "0",
    ])
    .assert()
    .failure();
}

#[test]
fn list_runs() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.arg("list").assert().success();
}

#[test]
fn preprocess_check_runs() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["preprocess", "check"]).assert().success();
}

#[test]
fn config_path_runs() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.args(["config", "path"]).assert().success();
}

#[test]
fn config_set_writes_and_get_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let path = path.to_str().unwrap();

    Command::cargo_bin("upio")
        .unwrap()
        .args(["--config", path, "config", "set", "gofile.server", "store4"])
        .assert()
        .success();

    let output = Command::cargo_bin("upio")
        .unwrap()
        .args(["--config", path, "config", "get", "gofile.server"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "store4");
}

#[test]
fn config_get_masks_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let path = path.to_str().unwrap();

    Command::cargo_bin("upio")
        .unwrap()
        .args([
            "--config",
            path,
            "config",
            "set",
            "pixeldrain.token",
            "pd-secret",
        ])
        .assert()
        .success();

    let masked = Command::cargo_bin("upio")
        .unwrap()
        .args(["--config", path, "config", "get", "pixeldrain.token"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&masked.stdout).trim(), "[REDACTED]");

    let shown = Command::cargo_bin("upio")
        .unwrap()
        .args([
            "--config",
            path,
            "config",
            "get",
            "--show-secrets",
            "pixeldrain.token",
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&shown.stdout).trim(), "pd-secret");
}

#[test]
fn config_get_suggests_nearby_keys() {
    let output = Command::cargo_bin("upio")
        .unwrap()
        .args(["config", "get", "bunkr.tokn"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("did you mean 'bunkr.token'"));
}

#[test]
fn unknown_command_fails() {
    let mut cmd = Command::cargo_bin("upio").unwrap();
    cmd.arg("bogus").assert().failure();
}

#[cfg(debug_assertions)]
#[test]
fn demo_upload_runs() {
    let output = Command::cargo_bin("upio")
        .unwrap()
        .args(["demo", "upload", "--files", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("upio upload"));
}

#[cfg(debug_assertions)]
#[test]
fn demo_download_runs() {
    let output = Command::cargo_bin("upio")
        .unwrap()
        .args(["demo", "download", "--files", "2"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("upio download"));
}

#[cfg(debug_assertions)]
#[test]
fn demo_mirror_runs() {
    let output = Command::cargo_bin("upio")
        .unwrap()
        .args(["demo", "mirror", "--files", "2"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("upio mirror"));
}
