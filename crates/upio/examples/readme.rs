use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("README.md");

    let readme = match std::fs::read_to_string(&path) {
        Ok(readme) => readme,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let synced = match upio::docs::sync(&readme) {
        Ok(synced) => synced,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    if synced == readme {
        println!("{} is up to date", path.display());
        return ExitCode::SUCCESS;
    }

    if check {
        eprintln!("{} is out of date; run `just readme`", path.display());
        return ExitCode::FAILURE;
    }

    match std::fs::write(&path, synced) {
        Ok(()) => {
            println!("updated {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}
