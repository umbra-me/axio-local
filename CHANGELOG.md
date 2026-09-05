# Changelog

## Unreleased

- September 5: Verified recovery failures, unique/private snapshots, overlapping-launch locks, named profiles and normalized profile URLs. See [change set](docs/change-set-2026-09-05.md).

- Rename the binary from `lmrun` to `axio-local`. The `LMRUN_*` environment
  variables and the `~/.config/lmrun` directory keep their names for now.
- Rewrite the launcher as a Rust crate with the same behaviour as the Python
  original: isolated configuration homes, snapshot-and-restore of the real
  configuration files, two model roles, server auto-detection including VM
  hosts, `--shared`, `--reseed`, `--dry-run`, and the `status`, `list`,
  `serve`, `env`, `reset` and `snapshots` commands. Adds unit tests for the
  config, URL detection, every harness builder and the guard, and a CI
  workflow on Ubuntu and macOS.
- Initial Python `lmrun`: launch Claude Code, Codex, OpenCode, Pi, or Axio
  against a local model server with an isolated configuration home and a
  hash-based restore of the real configuration files.
