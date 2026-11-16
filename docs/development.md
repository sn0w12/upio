# Development

## Workspace layout

| Crate                 | Purpose                                                                         |
| --------------------- | ------------------------------------------------------------------------------- |
| `crates/core`         | Shared types, traits, errors, HTTP helpers, the download engine, and preprocessing. |
| `crates/browser`      | Shared Chrome session, hidden desktop/Xvfb, and the one Chrome profile.         |
| `crates/uploader-*`   | Uploader implementations.                                                       |
| `crates/upio`         | Unified library: feature flags, the upload pipeline, and the service registry.  |
| `crates/config`       | Config model, layered env/file reader, typed keys, and atomic writes.           |
| `crates/cli`          | The `upio` command-line binary.                                                 |
| `crates/gui`          | The `upio-gui` Rune desktop app.                                                |

## Versioning

`upio-core`, `upio-browser`, the per-service `uploader-*` crates, and the `upio`
crate are released together from the workspace version. The `upio-config` and
`upio-cli` crates are versioned independently.

## Checks

The `justfile` wraps the common commands. Run `just setup` once after cloning,
then `just ci` for the full local check. `just` with no arguments lists every
recipe.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

CI runs the same checks plus `cargo check -p upio --no-default-features`.

## Generated documentation

The services table in the README and the strategies table in
`docs/preprocessing.md` are generated from the code. After adding a service or
preprocessing strategy, regenerate them:

```sh
cargo run -p upio --example readme --all-features
```

`cargo test -p upio --all-features` fails when a generated table and the code
disagree. The tables assume every feature, including `download`, is enabled.

## Release build

```sh
cargo build --release -p upio-cli
```

The release profile is configured in the workspace root.
