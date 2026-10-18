# Axio Local

Current source changes and verification gates: [September 5 change set](docs/change-set-2026-09-05.md).

Run any coding agent against models served on your own machine, without
touching the agent's real configuration.

`axio-local` launches Claude Code, Codex, OpenCode, Pi, or Axio with an isolated
configuration home and the environment variables that point it at a local
OpenAI- and Anthropic-compatible server. When the agent exits, the launcher
verifies that the agent's real configuration files are byte-identical to how it
found them, and restores any that changed.

It is a single Rust binary with no runtime dependencies.

## Install

```bash
cargo install --git https://github.com/umbra-me/axio-local --locked
```

or from a clone, `cargo install --path . --locked`. Either puts `axio-local` in
`~/.cargo/bin`.

The server is anything that serves `/v1/models`, `/v1/chat/completions`,
`/v1/responses` and `/v1/messages`. [oMLX](https://github.com/jundot/omlx) on
Apple silicon is the reference setup; see `docs/omlx.md`.

## Use

```bash
axio-local status                      # server reachability, which models are served
axio-local claude                      # Claude Code on the orchestrator model
axio-local codex exec "..."            # Codex on the worker model over the Responses API
axio-local opencode                    # OpenCode, orchestrator + small_model = worker
axio-local pi -p "..."                 # Pi
axio-local axio                        # Axio itself, openai-compatible provider
axio-local shell                       # a subshell; anything started in it inherits the env
axio-local shell 'aider --model x'     # or run one command in that env
eval "$(axio-local env)"               # the same env in the current shell
```

Options go before the harness name; everything after it is passed through:

| Option | Effect |
| --- | --- |
| `-o MODEL`, `--orchestrator MODEL` | the main model for this launch |
| `-w MODEL`, `--worker MODEL` | the model subagents and light tasks use |
| `--profile NAME` | named server/model profile from config.json |
| `--url URL` | server base URL; otherwise auto-detected |
| `--shared` | use the real configuration directory instead of an isolated one; restore on exit still applies |
| `--reseed` | re-copy `CLAUDE.md`, settings, skills, agents, and `AGENTS.md` into the isolated home |
| `--dry-run` | print the environment and generated configuration, launch nothing |

Two roles, because a dense model that is best at planning and a sparse model
that is fastest under concurrency are rarely the same file. Claude Code maps
`opus`, `sonnet` and `fable` to the orchestrator and `haiku` plus every
subagent to the worker; OpenCode uses the worker as its `small_model`; Codex,
Pi, and Axio are single-model and take the worker or orchestrator
respectively.

## What it protects

Each harness has its own way to relocate its home, and the launcher uses it:

| Harness | Relocation | Generated in the isolated home |
| --- | --- | --- |
| Claude Code | `CLAUDE_CONFIG_DIR` | `.claude.json` with onboarding marked complete; `CLAUDE.md`, `settings.json`, `skills/`, `agents/`, `commands/` copied from the real home on first run |
| Codex | `CODEX_HOME` | `config.toml` with an `omlx` provider on the Responses wire; `AGENTS.md` copied |
| OpenCode | `OPENCODE_CONFIG` | `opencode.json` with the provider and both models; the real global config still merges underneath, read-only |
| Pi | `PI_CODING_AGENT_DIR` | `models.json` with the provider and both models |
| Axio | `AXIO_HOME` | `config.toml` with `[model] provider = "openai-compatible"` |

Isolated homes live under `~/.config/lmrun/homes/<harness>`; `axio-local reset
<harness>` deletes one, and refuses while that harness is running.

Before every launch the real configuration files the harness could touch are
hashed and copied to `~/.config/lmrun/snapshots/<timestamp>-<harness>-<pid>-<counter>`. On
exit, including Ctrl+C, SIGTERM and SIGHUP, any file whose hash changed or
that disappeared is restored from the snapshot and named on stderr. The last
ten completed snapshots are kept; failed and pending snapshots are retained.
Restore failures return a nonzero exit status and identify the retained backup. `axio-local snapshots` lists them.

The launcher runs the agent as a child process and installs its own SIGINT
handler, so Ctrl+C reaches the agent and the launcher survives to do the
restore. SIGTERM and SIGHUP are forwarded to the agent once.

## Configuration

`~/.config/lmrun/config.json`, written with defaults on first run:

```json
{
  "url": null,
  "token": "local",
  "orchestrator": "Jundot--Qwen3.8-27B-oQ4e-mtp",
  "worker": "gpt-oss-120b-MXFP4-Q8",
  "codex_model": "gpt-oss-120b-MXFP4-Q8",
  "context_window": 131072,
  "max_output": 32768,
  "keep_snapshots": 10
}
```

`url: null` means auto-detect: `127.0.0.1:8000` first, then `host.internal`,
`host.orb.internal` and `host.docker.internal` for a VM or container whose
host runs the server. `LMRUN_URL` overrides both. On macOS, when the server is
local and `~/.omlx/start.sh` exists, a launch starts it if it is down.

Model names must match what the server reports from `/v1/models`; a launch
warns when a configured role names a model the server does not serve.

Use `--profile NAME` before the harness to select an entry in the optional
`profiles` object. Each entry supplies `url`, `orchestrator`, and `worker`;
the worker also becomes the Codex model. Explicit `--url` takes precedence
over the profile, and a selected profile takes precedence over `LMRUN_URL`.
Trailing slashes are removed from the chosen URL before API paths are appended.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

Unit tests cover the config file, URL detection, every harness builder, and
the snapshot guard's restore, resurrect and prune paths. What they cannot
cover is a real agent talking to a real server; that is the launch matrix
under Status.

## Status

Verified on 2026-09-03 against oMLX 0.6.4 on an M5 Max: every harness above
launched, used a tool, and exited with its real configuration verified
untouched; the shared mode restore and the tamper restore were exercised
deliberately. Codex used the Responses API. Claude Code was verified inside an
OrbStack Ubuntu VM reaching the host server at `host.internal`, and routed a
parallel fan-out of three subagents to the worker model.

## License

Apache-2.0. See `LICENSE` and `NOTICE`.
