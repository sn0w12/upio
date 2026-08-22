set shell := ["pwsh", "-NoLogo", "-Command"]
set dotenv-load := false

default: check

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy on the whole workspace with all targets/features (like CI)
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run clippy but allow warnings
clippy-loose:
    cargo clippy --workspace --all-targets --all-features

# Type-check the whole workspace
check:
    cargo check --workspace

# Type-check everything with all targets and features
check-all:
    cargo check --workspace --all-targets --all-features

# Run all tests
test:
    cargo test --workspace

# Build everything in release mode
build:
    cargo build --workspace --release

# Bundle GUI for windows
bundle-win:
    dx bundle -p "upio-gui" --desktop --package-types "msi"

# Run the CLI, passing any args through, e.g. `just cli list`
cli *ARGS:
    cargo run -p upio-cli -- {{ARGS}}

# Run the desktop GUI
gui:
    dx serve -p upio-gui

# Full check pipeline: fmt, clippy, test
ci:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace

# Fix what can be fixed automatically (fmt + clippy autofix)
fix:
    cargo fmt --all
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
