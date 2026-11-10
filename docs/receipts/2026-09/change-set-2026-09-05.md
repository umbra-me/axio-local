# September 5 change set

## Recovery and named profiles — 2026-09-05

Snapshots use nanoseconds, process ID and a counter; creation refuses reuse.
Copies are hash-checked, and Unix snapshot directories are private to the user.
OS file locks cover generated homes and protected real paths throughout launch
and recovery. Overlapping cooperating launches fail before configuration generation;
locks release when the process exits. They do not prevent unrelated programs from
editing the same files and do not provide automatic recovery after SIGKILL.

Recovery counts verified files, verifies restored bytes, reports failures and
returns a failing exit status. Failed and pending snapshots survive retention;
only completed snapshots are pruned. Newly created real configuration files are
reported and left in place. `keep_snapshots: 0` disables pruning.

`--profile NAME` selects a `profiles` entry with `url`, `orchestrator`, and `worker`.
The worker also sets the Codex model. Explicit role flags override profile roles;
`--url` wins over the selected profile, then normal environment/config detection
applies when no profile URL was selected. URL trailing slashes are normalized
before API paths are appended. Profiles require HTTP and nonempty model roles.

Example addition to `~/.config/lmrun/config.json`:

```json
{"profiles":{"workstation":{"url":"http://localhost:8000/","orchestrator":"YOUR_PLANNER","worker":"YOUR_WORKER"}}}
```

Run `axio-local --profile workstation axio`; with the companion CLI installed,
`axio local workstation` performs the same handoff. Configuration paths and the
`LMRUN_*` environment names remain compatible. Source version remains 0.1.0.

Verification: 19 tests, formatting, Clippy with denied warnings and the locked
release build passed on macOS. A live named-profile agent/tool session and a
post-change real configuration recovery exercise remain to be performed.
