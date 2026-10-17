# Developing `axio/local` with Umbra

Use `umbra go --list` to find registered folders; `umbra go <component-id>` jumps
to a product after shell setup (`eval "$(umbra shell-init zsh)"` in an existing
zsh terminal). `umbra path <component-id>` prints the folder for scripts.
The same shell hook enables live Tab completion for `go`, `dev` and `stop`;
source suggestions follow the current repository scope or explicit `--root`.
Source shortcuts are `umbra dev` and `umbra stop` when a routed preview is declared;
use `umbra dev --list` to inspect availability. `--status`, `--logs`, `--doctor`,
`--plan`, `--restart` and `--open` are available on `umbra dev`. See the availability guidance
below for this repository's routed-preview support; native/backend workflows
retain their existing gates.


This independent repository owns its source, dependency locks and install roots.
Read [AGENTS.md](AGENTS.md) and [umbra.dev.yaml](umbra.dev.yaml) before running
actions. Nested subtree instructions still apply; Umbra does not replace them.

## Select and inspect

Use an Umbra CLI build exposing `products dev-list` and `products capabilities`.
From the company root, run `cargo build --manifest-path control-plane/cli/Cargo.toml --locked` and add its
`target/debug` directory to this shell's PATH. This does not install the operator
CLI. From this repository:

```sh
umbra products --root . list
umbra products --root . fingerprint axio/local
umbra --env dev products --root . dev-list
```

## Declared source actions

Inspect the recipe first, then execute only the action authorized for the task.

```sh
umbra products --root . check axio/local
umbra products --root . build axio/local
```

The recipe selects this repository's tools and working directories. Prepare
actions install dependencies and require the task's installation authorization.
Keep installations in the declared product install root; company, brand and
family references are catalogues. Keep secrets outside source. Native launch,
provider/database integration, publication and operational actions retain their
existing gates. A source check does not establish those acceptance results.

## Web source development

This recipe currently declares no managed routed web preview. `dev-list`
therefore lists no source web endpoint for this component. Do not invent a
hostname/port, use an operational container-replacement command, or launch a
native app to fill that gap. Use the source actions above and this repository's
existing native/backend/package development instructions. A managed HTTP
service needs an explicit routed preview in the owned recipe first.

The company guide is `docs/local-development.md`; the detailed control-plane
guide is `docs/development/local-source.md`. Those repositories own orchestration
and the shared source profile. This document keeps the essential local commands
available when an agent opens only this independent repository.
