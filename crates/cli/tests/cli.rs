use assert_cmd::Command;

#[test]
fn help_smoke() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.arg("--help").assert().success();
}

#[test]
fn version_flag() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.arg("--version").assert().success();
}

#[test]
fn upload_help() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.args(["upload", "--help"]).assert().success();
}

#[test]
fn upload_without_paths_fails() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.arg("upload").assert().failure();
}

#[test]
fn list_runs() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.arg("list").assert().success();
}

#[test]
fn preprocess_check_runs() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.args(["preprocess", "check"]).assert().success();
}

#[test]
fn config_path_runs() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.args(["config", "path"]).assert().success();
}

#[test]
fn unknown_command_fails() {
    let mut cmd = Command::cargo_bin("upio-cli").unwrap();
    cmd.arg("bogus").assert().failure();
}
