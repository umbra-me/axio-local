# Axio Local (`lmrun`)

One Python file, `lmrun`, that launches a coding agent against a local model
server with an isolated configuration home and restores the agent's real
configuration on exit. Optional component `axio/local` of the Axio workspace:
it installs onto a workstation and deploys nothing, like Deck, Analyst and
Capture.

## Commands

```bash
python3 -m py_compile lmrun          # the only build step; must pass on Python 3.9
./lmrun --help                       # usage
./lmrun --dry-run <harness>          # print env + generated config without launching
./lmrun status                       # is a server up, which models does it serve
```

There is no test suite yet. Verification is the launch matrix in
`README.md` under Status: run each harness, confirm it used a tool, and
confirm the closing `real configs untouched` line. The restore path is
exercised with `lmrun shell 'echo x >> ~/.claude/settings.json'`, which must
end with a `RESTORED` line and an unchanged file.

## Architecture

- `PROTECTED` maps a harness to the real files it may touch. Adding a harness
  means a `build_<name>` function returning `(env, extra_argv, notes)`, an
  entry in `BUILDERS`, `HARNESSES` and `PROTECTED`, and a line in the
  docstring and `README.md`.
- Isolation comes from each harness's own relocation variable
  (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_CONFIG`,
  `PI_CODING_AGENT_DIR`, `AXIO_HOME`). Never write into a real config
  directory; generated files go under `~/.config/lmrun/homes/<harness>`.
- `Guard` is the safety net: hash and copy `PROTECTED` files before launch,
  rehash after, restore on mismatch. It runs in `run_child`'s `finally`, so it
  must stay cheap and must not raise.
- The launcher keeps Python 3.9 compatibility because that is macOS's bundled
  interpreter: no `match`, no `X | Y` types, no `tomllib`. TOML is emitted as
  strings on purpose.
- Model roles are `orchestrator` and `worker`. Every harness-specific mapping
  is derived from those two names, never hard-coded per model.

## Gotchas

- `pkill -f` and `pgrep -f` match their own caller when the pattern appears in
  its command line. The launcher never uses them; anything that inspects
  processes must exclude itself.
- Claude Code logs `unrecognized_model` for a local model name. It is
  cosmetic. `-p` without stdin redirected waits three seconds for piped
  input; use `< /dev/null` in scripts.
- oMLX ignores `--hot-cache-max-size 20%`; it wants a size such as `16GB`.
- Hugging Face's xet transport stalled to near zero on the reference network;
  `HF_HUB_DISABLE_XET=1` restored full speed. That is a download concern, not
  a launcher one, but it is where a new user gets stuck first.

## Definition of done

- `py_compile` passes on 3.9 and the affected harness launches, uses a tool,
  and closes with `real configs untouched`.
- `README.md` and the docstring describe the same options and harnesses.
- Do not commit, push, tag, or release unless asked.
