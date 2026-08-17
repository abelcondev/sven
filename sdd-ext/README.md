# grok-sdd — Spec-Driven Development layer for grok-build

> **⚠️ LEGACY — being replaced by a native Rust integration.** This external Go
> binary + hooks layer is superseded by the in-tree crate `crates/common/xai-grok-sdd`
> and the native `sdd` tool. See `docs/NATIVE_SDD_INTEGRATION.md`. Notably, the **Stop
> hook described below was removed** (Phase 3): its per-turn `additionalContext` made
> grok treat every turn as "keep working", forcing autonomous continuation — the cause
> of the runaway on a plain greeting. The loop's next-step is now surfaced on demand via
> the passive standing rules and the `sdd` tool's own outputs, neither of which forces
> continuation. This directory is deleted in Phase 6.

This directory adds a full **Spec-Driven Development (SDD) + unified loop** to the
grok-build agent, ported from `kez`. It is **additive**: it changes no grok-build
Rust core. Everything works through grok's existing extension points — skills,
project rules, hooks, and the terminal tool.

There is **no MCP server** and no slash commands. The agent runs the `grok-sdd`
binary through its normal terminal tool, guided by the injected rules and skills —
exactly the way kez drives `kez sdd`. You talk to grok in plain language; it runs
the right command.

## What you get

The one loop covers everything — discovery, stack choice, design-system
foundations, and every feature:

```
propose → approve → [design → approve-design, for UI] → task → branch →
implement (TDD) → review → ship → merge → cleanup → repeat
```

- **Durable state on disk** under `sdd/` (proposals, decisions, designs, tasks,
  log) — survives context compaction; the loop position is recomputed from disk +
  the git branch every turn.
- **One next step, always.** `grok-sdd next` prints exactly one recommended action.
- **Human gates.** Proposal approval and design review stop and hand control back.
- **Branch & PR policy**, enforced by a compiled branch guard (a `PreToolUse` hook)
  that refuses code writes on the protected branch.
- **Task tiers** scale review ceremony to risk (trivial / standard / critical).
- **9 dense phase skills** the agent loads on demand.

## Architecture (how it plugs into grok-build)

| Piece | grok-build mechanism | What it does |
|---|---|---|
| `engine/` → `grok-sdd` binary | the terminal tool | the whole SDD engine + git/gh glue (Go, vendored from kez, no runtime deps) |
| `grok/skills/sdd-*/SKILL.md` | `~/.grok/skills/` (on-demand skills) | the dense rules for each loop phase |
| `.grok/rules/sdd.md` (written by `grok-sdd init`) | project rules (injected every turn) | the standing policy: loop, gates, branch/PR policy, route advisor |
| `grok-sdd hook stop` | `Stop` hook (`additionalContext`) | injects the current single next step after each turn, only in SDD projects |
| `grok-sdd hook pretooluse` | `PreToolUse` hook (blocking gate) | the branch guard: denies code writes on the protected branch |

The only functional feature **not** portable without recompiling grok-build's Rust
is the implement-phase turn-budget widening (kez §12) — deliberately deferred.

## Install

One line — downloads a prebuilt binary + the skills/hooks bundle from the latest
GitHub release. No Go, no clone, no kez:

```sh
curl -fsSL https://raw.githubusercontent.com/abelcondev/sven/main/sdd-ext/bootstrap.sh | bash
```

Overridable with `GROK_HOME`, `BIN_DIR`, `GROK_SDD_VERSION`.

### From source

```sh
make build          # needs Go
./install.sh
```

`install.sh` installs the binary to `~/.local/bin`, skills to `~/.grok/skills/`,
and hooks to `~/.grok/hooks/sdd.json`. A prebuilt binary in `./dist` is used if
present; otherwise it builds with `go`.

### Cutting a release (maintainer)

```sh
make release-assets                               # dist/ binaries + assets tarball
gh release create vX.Y.Z sdd-ext/dist/grok-sdd-*  # upload as release assets
```

The engine is self-contained and does **not** depend on kez, so the release
works on any teammate's machine.

## Use

In any repo you want under SDD:

```sh
grok-sdd init      # scaffolds sdd/, enables the branch guard, writes .grok/rules/sdd.md
```

Then just talk to grok. It reads `sdd/index.md`, follows the loop, stops at gates,
and runs `grok-sdd` commands for you. Any time, `grok-sdd next` / `grok-sdd status`
show where the loop is. Non-SDD repos are untouched — the hooks no-op when there is
no `sdd/`.

### Commands (grok runs these; you rarely type them)

```
grok-sdd init                        Scaffold sdd/ + branch guard + rules (idempotent)
grok-sdd propose "<what & why>"      Branch + seed sdd/proposal.md for the agent to expand
grok-sdd approve [--title <text>]    Promote proposal.md → decisions/NNN
grok-sdd design <decision> <title>   Scaffold an in-review UI design (the gate before UI code)
grok-sdd approve-design <design>     Approve a design, unblocking its decision's UI tasks
grok-sdd task <decision> <title> [--tier trivial|standard|critical]
grok-sdd done <task> [--residual …]  Mark done; each --residual → a follow-up task
grok-sdd ship <task> [--residual …]  Pre-flight (branch/remote/gh) then close
grok-sdd cleanup [--dry-run]         Delete merged proposal branches
grok-sdd status | next               Report state / print the single next step
grok-sdd guard <path>                Branch-guard check (used by the hook)
grok-sdd hook stop | pretooluse      Hook entry points (used by ~/.grok/hooks/sdd.json)
```

## Layout

```
sdd-ext/
  engine/              Go module (grok-sdd binary)
    main.go            CLI + hook modes
    assets/rules_sdd.md  embedded .grok/rules template
    internal/          vendored from kez: sdd, route, branchguard, qualitygate
  grok/skills/sdd-*/   the 9 phase skills (installed to ~/.grok/skills)
  hooks/sdd.json       Stop + PreToolUse hook definitions (installed to ~/.grok/hooks)
  install.sh  Makefile  README.md
```
