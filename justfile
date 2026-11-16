[windows]
set shell := ['pwsh', '-NoLogo', '-NoProfile', '-CommandWithArgs']
[unix]
set shell := ['sh', '-cu']
set positional-arguments
set default-list

# Install developer tools and generated GUI assets (run once after cloning)
[group('setup')]
setup: gui-install gui-css
    rustup component add clippy rustfmt

# Format all Rust code
[group('check')]
fmt:
    cargo fmt --all

# Check formatting without modifying files
[group('check')]
fmt-check:
    cargo fmt --all -- --check

# Lint the whole workspace with all targets and features
[group('check')]
clippy: gui-css
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Type-check the workspace
[group('check')]
check: gui-css
    cargo check --workspace --all-targets

# Type-check the workspace with every feature
[group('check')]
check-all: gui-css
    cargo check --workspace --all-targets --all-features

# Check the library without default features
[group('check')]
check-no-default:
    cargo check -p upio --no-default-features

# Run the workspace tests
[group('check')]
test: gui-css
    cargo test --workspace

# Run the download-feature tests
[group('check')]
test-download:
    cargo test -p upio-core --features download
    cargo test -p upio --all-features

# Run the full local CI: setup, formatting, lint, tests, generated docs
[group('check')]
ci: setup fmt-check clippy check-no-default test test-download docs-check

# Fix formatting and clippy issues automatically
[group('check')]
fix: gui-css
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt --all

# Build the workspace in debug mode
[group('build')]
build: gui-css
    cargo build --workspace

# Build the release CLI
[group('build')]
release:
    cargo build --release -p upio-cli

# Remove build artifacts
[group('build')]
clean:
    cargo clean

# Run the CLI with arguments, e.g. `just cli download <url>`
[group('run')]
[windows]
cli *args:
    cargo run -p upio-cli -- $(if ($args.Count -gt 1) { $args[1..($args.Count - 1)] })

[group('run')]
[unix]
cli *args:
    cargo run -p upio-cli -- "$@"

# Install the GUI's JavaScript dependencies
[group('gui')]
gui-install:
    npm --prefix crates/gui ci --no-audit --no-fund

# Build the GUI's Tailwind stylesheet
[group('gui')]
gui-css:
    npm --prefix crates/gui run build:css

# Run the desktop GUI (run `just setup` once first)
[group('run')]
gui: gui-css
    dx serve -p upio-gui --desktop

# Bundle the GUI as a Windows MSI (run `just setup` once first)
[group('gui')]
bundle-win: gui-css
    dx bundle -p upio-gui --desktop --package-types msi

# Regenerate the generated documentation tables
[group('docs')]
docs:
    cargo run -p upio --example readme --all-features

# Check that generated documentation is current
[group('docs')]
docs-check:
    cargo run -p upio --example readme --all-features -- --check
