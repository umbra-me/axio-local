# Development

Install Rust 1.98.1 with rustfmt and clippy. From this repository run `cargo fetch --locked`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, and `cargo build --release --locked`. CI uses the same toolchain and commands on Ubuntu and macOS. Tests use synthetic temporary files and require no running agent or model server. No preview service is declared.

`umbra.dev.yaml` describes these native commands for optional orchestration. Agent launches, tool use, actual configuration restoration and Windows execution remain separate manual acceptance gates. Do not use a real configuration home for migration tests.
