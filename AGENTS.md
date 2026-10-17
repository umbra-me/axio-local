# Axio Local (crate and binary: `axio-local`)

A Rust CLI that launches a coding agent against a local model server with an
isolated configuration home and restores the agent's real configuration on
exit. Optional component `axio/local` of the Axio workspace: it installs onto
a workstation and deploys nothing, like Deck, Analyst and Capture.

## Commands

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked                                   # unit tests; no network, no agents
cargo build --release --locked                        # target/release/axio-local
cargo install --path . --locked                       # ~/.cargo/bin/axio-local
axio-local --dry-run <harness>                             # env + generated config, launches nothing
axio-local status                                          # is a server up, which models does it serve
```

CI (`.github/workflows/ci.yml`) runs exactly the first four on Ubuntu and
macOS. Verification beyond the tests is the launch matrix in `README.md` under
Status: run each harness, confirm it used a tool, and confirm the closing
`real configs untouched` line. The restore path is exercised with
`axio-local shell 'echo x >> ~/.pi/agent/models.json'`, which must end with a
`RESTORED` line and a byte-identical file.

## Architecture

- `src/main.rs` is the clap surface and dispatch. Global options are declared
  `global = true` but documented as "before the harness name", because every
  harness subcommand takes `trailing_var_arg` with `allow_hyphen_values` and
  swallows anything after it.
- `src/harness.rs` holds one builder per harness returning a `Launch` (env to
  set, variables to unset, arguments to prepend, notes). Adding a harness
  means a variant in `Harness`, its `name`, `protected` list and
  `generated_files`, a `build_*` function, a `Cmd` variant in `main.rs`, and a
  row in `README.md`.
- Isolation comes from each harness's own relocation variable
  (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_CONFIG`,
  `PI_CODING_AGENT_DIR`, `AXIO_HOME`). Builders never write into a real
  configuration directory; generated files go under
  `~/.config/lmrun/homes/<harness>`.
- `src/guard.rs` is the safety net: hash and copy `protected` files before
  launch, rehash after, restore on mismatch or deletion. `restore` never
  returns an error, because it runs after the harness exits and a partial
  restore reported beats a panic that reports nothing.
- `src/launch.rs` spawns the child and installs SIGINT/SIGTERM/SIGHUP handlers
  through `signal-hook`. Installing a SIGINT handler is what keeps the parent
  alive when the terminal sends Ctrl+C to the whole foreground group; SIGTERM
  and SIGHUP are forwarded once with `libc::kill`.
- `src/server.rs` detects the server URL (`LMRUN_URL`, config, localhost, then
  the VM host names), lists models over plain HTTP with `ureq` (no TLS
  feature; the server is on this machine or its host), and on macOS starts
  `~/.omlx/start.sh` when the URL is local and nothing answers.
- Model roles are `orchestrator` and `worker`. Every harness-specific mapping
  derives from those two names; nothing is hard-coded per model.
- `--profile NAME` selects server and model roles from `config.json` profiles.
  `Opts::url` normalizes the selected endpoint before API paths are appended;
  explicit `--url` wins over a profile, which wins over `LMRUN_URL`.
- TOML for Codex and Axio is emitted as a string on purpose: two files, a
  handful of keys, and a `toml` dependency would be the largest thing in the
  crate.

## Gotchas

- `time` is used for the snapshot timestamp, not `chrono`, to match the Axio
  family; the stamp is UTC so no local-offset feature is needed.
- Claude Code logs `unrecognized_model` for a local model name. It is
  cosmetic. `-p` without stdin redirected waits three seconds for piped
  input; use `< /dev/null` in scripts.
- oMLX ignores `--hot-cache-max-size 20%`; it wants a size such as `16GB`.
- Hugging Face's xet transport stalled to near zero on the reference network;
  `HF_HUB_DISABLE_XET=1` restored full speed. A download concern, not a
  launcher one, but it is where a new user gets stuck first.

## Definition of done

- fmt, clippy with `-D warnings`, tests and a locked release build pass, and
  the affected harness launches, uses a tool, and closes with
  `real configs untouched`.
- `README.md`, the clap help text and this file describe the same options and
  harnesses.
- Do not commit, push, tag, or release unless asked.

## Umbra CLI entry point

Read [DEVELOPMENT.md](DEVELOPMENT.md) for this repository's exact Umbra CLI
source actions and routed-preview availability. Use its stable component ID
`axio/local`; installs remain in the owned recipe's install root. Preserve
all existing native, provider, data, publication and production constraints.
