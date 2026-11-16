use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let targets = [
        (root.join("README.md"), &["services"][..]),
        (root.join("docs/preprocessing.md"), &["strategies"][..]),
    ];

    let mut failed = false;
    for (path, blocks) in targets {
        if let Err(error) = sync_file(&path, blocks, check) {
            eprintln!("{error}");
            failed = true;
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn sync_file(path: &Path, blocks: &[&str], check: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let synced = upio::docs::sync_blocks(&text, blocks).map_err(|error| error.to_string())?;

    if synced == text {
        println!("{} is up to date", path.display());
        return Ok(());
    }

    if check {
        return Err(format!(
            "{} is out of date; run `just readme`",
            path.display()
        ));
    }

    std::fs::write(path, synced)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("updated {}", path.display());
    Ok(())
}
