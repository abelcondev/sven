# Native SDD integration into grok-build (Rust)

Plan to move the SDD loop from the external `sdd-ext/` layer (Go binary + JSON hooks
+ rules files) into grok-build natively, in Rust.

**Decisions locked (2026-08-16):**
- **Go straight to native.** No interim Go-side quick fix. The runaway-control fix
  lands once, natively (Phase 3).
- **Accept divergence from the monorepo sync.** We edit `crates/` freely and stop
  taking automatic `Synced from monorepo` updates. Upstream grok-build changes are
  pulled manually/cherry-picked when wanted, not auto-synced.

---

## Why we're doing this

Two real complaints drove it:

1. **Loss of control** — saying "hola" kicked off SDD ceremony. Root cause is the
   injection wiring, not the engine language: the Stop hook (`grok-sdd hook stop`,
   `sdd-ext/engine/main.go:508`) injects a "next step" **every turn** whenever
   `sdd/index.md` exists — it never checks whether the user actually engaged the
   loop. In a freshly-`init`ed project `LoopState.Next()` returns *"draft the first
   proposal, load sdd-discovery"*, so every message (incl. "hola") nudges ceremony.
2. **Slowness** — a subprocess per Stop turn plus the model running multi-step
   ceremony and loading dense skills (a consequence of #1).

### What native Rust genuinely buys (verified against the crates)

- **§12 turn-budget widening** during the implement phase, via
  `AgentDefinition::max_turns` — the one feature that was *impossible* in the
  additive layer and stayed deferred.
- **Stateful gating** of injection — the control fix — with access to real session
  state, not just disk presence.
- **Single-binary distribution** — no separate `grok-sdd` Go binary, no Go
  toolchain, no `make crosscompile` for friends.
- No per-turn subprocess.

### What it does NOT change

Per-turn context injection still flows through the **same** `Stop`-gate
`additionalContext` channel that the Go hook uses today — there is no pre-turn
callback API in grok-build. "Native injection" means *the decision logic* is
native (in-process, stateful), not a new injection mechanism.

---

## Architecture map (grok-build extension points)

| SDD concern | Native hook-in point |
|---|---|
| Per-turn "next step" injection | `Stop`-gate contributor in `xai-grok-shell/src/session/acp_session_impl/stop_gate.rs` (returns `additionalContext`; `MAX_STOP_HOOK_CONTINUATIONS_PER_TURN = 8`) |
| Block a code write off a protected branch | Pre-tool check in the edit-tool dispatch path (`xai-grok-tools`), replacing the `PreToolUse` hook |
| Native `sdd` built-in tool | `register_tool_pack()` / `b.register::<T>()` in `xai-grok-tools/src/registry/types.rs` |
| Standing rules (every turn) | `.grok/rules/*.md` → `<always_applied_workspace_rules>` in `xai-grok-agent/src/prompt/user_message.rs` |
| Phase skills (one-time, at session build) | `.grok/skills/<name>/SKILL.md`, discovered in `xai-grok-agent/src/prompt/skills.rs` |
| Turn budget (§12) | `AgentDefinition::max_turns`, resolved in `xai-grok-shell/src/session/acp_session_impl/agent_ops.rs` |

Engine to port: `sdd-ext/engine/` — ~2,700 non-test LOC. Core is a **pure function**
`LoopState.Next()` (`internal/sdd/loop.go`) derived from `sdd/` files + git branch.
Packages: `internal/sdd` (1242), `internal/branchguard` (187), `internal/qualitygate`
(145), `internal/route` (58), plus `main.go` (1094, mostly CLI glue we replace).

---

## Phased plan

### Phase 1 — Fork hygiene + engine crate `xai-grok-sdd` — ✅ DONE (2026-08-16)
Goal: the SDD engine exists in Rust with parity tests, touching nothing else yet.

**Status:** `crates/common/xai-grok-sdd/` built and added to the workspace members.
Ported modules: `tier`, `qualitygate`, `route`, `util` (frontmatter/slug/numbering),
`scaffold` (+ embedded `templates/` via `include_str!`), `loop_state`
(`read_loop_state`/`LoopState::next`), `lifecycle` (promote/add_task/complete_task/
add_design/approve_design), `propose` (seed_proposal/proposal_branch_name),
`branchguard`. **55 tests green** (1:1 port of the Go `*_test.go` suite — this IS the
parity gate), `cargo clippy` clean, `cargo fmt` clean. Deviation from the plan: the
throwaway `xai-grok-sdd-cli` parity bin was **not** built — the ported test suite
(identical assertions to the Go tests) already guarantees parity, so a CLI diff harness
was redundant. Engine advisory strings still say `grok-sdd <sub>`; Phase 2 rewrites the
skills to drive the native tool instead.

- Stop the monorepo auto-sync (document the cutoff commit; keep a note on how to
  cherry-pick upstream later).
- New crate `crates/common/xai-grok-sdd/` — pure logic + filesystem, no CLI:
  - Port `LoopState`, `NextAction`, `ReadLoopState`, `Next` (`loop.go`).
  - Port lifecycle: `Scaffold`/init, `propose`, `approve`, `task`, `design`,
    `approve-design`, `done`, `ship`, `preflight`, `cleanup`, `status`
    (`sdd.go`, `lifecycle.go`, `propose.go`, `tier.go`).
  - Port `branchguard`, `route`, `qualitygate`.
  - Templates via `include_str!` (from `internal/sdd/templates/`).
- Port the Go table tests (`*_test.go`) as Rust tests — this is the parity gate.
- Keep a throwaway `xai-grok-sdd-cli` bin during migration to diff behavior against
  the Go `grok-sdd`; delete it at the end of Phase 6.

**Deliverable:** `cargo test -p xai-grok-sdd` green, behavior matches Go binary.

### Phase 2 — Native `sdd` tool — ✅ DONE (2026-08-16, except 2b)
- Implement `SddTool` (one tool, structured input `{action, args[], title?, tier?,
  residual[]}`) implementing `xai_tool_runtime::Tool` + `ToolMetadata`.
- Register it in `xai-grok-tools/src/registry/types.rs`.
- Rewrite the 9 phase skills (`sdd-ext/grok/skills/sdd-*/SKILL.md`) to drive the
  `sdd` tool instead of shelling `grok-sdd <sub>`.

**Deliverable:** the agent runs the loop through the native tool; no `grok-sdd`
subprocess.

**Status:** Engine gained a `cli` module (`xai-grok-sdd/src/cli.rs`) — a
model-facing façade that maps an action to an engine call and returns text (the Go
`main.go` CLI, ported, tool-oriented; `grok-sdd X` next-step hints are rendered as
`sdd` tool actions). Tool lives at `xai-grok-tools/src/implementations/sdd/mod.rs`
(`SddTool`, id `sdd`, `kind = ToolKind::Other` so it's non-read-only, gets `cwd` via
`resolve_cwd`). Wired in: added `Sdd(SddInput)` to the `ToolInput` enum
(`types/tool_io.rs`), a `return None` arm in `normalization.rs`, `pub mod sdd` in
`implementations/mod.rs`, path dep in `xai-grok-tools/Cargo.toml`, and
`b.register::<SddTool>()` in the registry. **60 engine tests + 3009 xai-grok-tools
tests green**, clippy + fmt clean. Skills rewritten to call the tool.

**Phase 2b — DROPPED (not needed).** `ship`/`preflight`/`cleanup` were compiled into
kez's `grok-sdd` binary only to **escape kez's sandbox**, which blocked git/gh. grok-build
has no such sandbox — the agent runs `git push` / `gh pr` / merge natively via bash. So
re-porting that orchestration would recreate a workaround that isn't needed here. The
current state is the correct FINAL design: the `sdd` tool returns the git/gh steps and the
agent runs them with its native tools; `done` covers the one state mutation `ship` wraps.
No further work on ship/preflight/cleanup.

### Phase 3 — Kill the runaway (**the control fix**) — ✅ DONE (2026-08-16)

**Key finding that reframed this phase.** The runaway was NOT merely "unconditional
injection" — it was the **injection mechanism forcing continuation**. In grok-build,
`StopDispatchResult::wants_continuation()` returns `true` whenever `additional_context`
is non-empty (`xai-grok-hooks/src/dispatcher.rs`). The Go Stop hook (`grok-sdd hook
stop`) returned `additionalContext` **every turn** an SDD project was present, so grok
treated every turn — including a plain "hola" — as "keep working" and re-entered the
model autonomously, up to `MAX_STOP_HOOK_CONTINUATIONS_PER_TURN = 8` times. That is the
"said hola and it started doing a series of things without control."

So the plan's original idea — "a native Stop-gate contributor that injects a `next:`
step" — is the **wrong mechanism**: the Stop-gate `additionalContext` path is for "you're
not done, keep working," never for a passive reminder. The correct native design is to
**not inject on Stop at all**:

- **Removed the SDD Stop hook** (`sdd-ext/hooks/sdd.json`) — this is THE fix. No SDD
  path forces continuation anymore. (PreToolUse branch-guard hook stays until Phase 4.)
- **Slimmed + reoriented `.grok/rules/sdd.md`** (`xai-grok-sdd/assets/rules_sdd.md`,
  written by `sdd init`) — the passive per-turn standing context, which does NOT force
  continuation. It now reads as **opt-in**: the loop engages only on explicit user
  request or an active proposal; a greeting/question/small fix is answered directly. It
  points the agent at the `sdd` tool (`next`/`status`) to recover loop position on
  demand.
- **On-demand next-step, not forced.** During active loop work the `sdd` tool's own
  outputs (`done`/`status`/`next`) already name the next step, so the agent stays guided
  without any forced injection. Structurally cannot run away.

**Deliverable:** "hola" in an SDD project does nothing — the agent answers and stops.
The loop engages only on explicit request or an active proposal. No Rust core change was
needed; the fix is removing the wrong-mechanism hook + slimming the passive rules.

**Optional future enhancement (not done):** a *passive* per-turn next-step nudge (when
the loop is in-flight) via the prompt-assembly reminder path (like the TodoNudge), which
appends a `<system-reminder>` without forcing continuation. Deferred — it needs
xai-grok-shell prompt-assembly surgery and the tool-output next-steps already cover the
need.

### Phase 4 — Native branch guard — ✅ DONE (2026-08-16)
- Move the `PreToolUse` branch guard into a native pre-execution check in the
  edit-tool dispatch path. Fail-open, same semantics as `branchguard::check`.

**Status:** Added a native guard in `xai-grok-shell/src/session/acp_session_impl/
tool_calls.rs::prepare_tool_call`, right after `access_kind` is computed and before
the plan-mode gate. When the resolved call is `AccessKind::Edit(path)` (every edit
tool — `search_replace`, `write`, `edit`, `str_replace`, hashline; `apply_patch`
carries a placeholder path and so is not path-gated, matching its AccessKind), it
calls `xai_grok_sdd::branchguard::check(cwd, path)`; on a `GuardError` it denies via
the existing `deny_tool(..)` path with hook name `sdd-branch-guard`. Added
`xai-grok-sdd` as a path dep of `xai-grok-shell`. Removed the `PreToolUse` entry from
`sdd-ext/hooks/sdd.json` (now `"hooks": {}`) so there's no double-guard. Fail-open: a
no-op outside opted-in repos (needs `.grok-sdd/require-branch` or
`GROK_SDD_REQUIRE_BRANCH=on`) and for non-code paths. `xai_grok_tools::ToolInput::Sdd`
resolves to `AccessKind::Read(None)` via the existing catch-all, so the `sdd` tool
itself is never branch-gated (and its writes are markdown anyway).

**Verification:** `xai-grok-shell` lib builds clean, clippy + fmt clean; the guard
logic is covered by the engine's 11 `branchguard` tests. (The shell's *test* binary
can't compile due to a **pre-existing** `base64::Engine`-import bug in a
synced-from-monorepo test file, `tool_layer_images_bridge_tests.rs` — unrelated to
this change; not fixed here to avoid touching unrelated synced code.)

### Phase 5 — §12 turn-budget widening
- Detect the implement phase from `LoopState` and bump `max_turns` for that turn
  where it's resolved (`agent_ops.rs`), so TDD red→green→refactor→review isn't
  cut off mid-cycle.

**Deliverable:** implement-phase turns get the wider budget; other phases unchanged.

### Phase 6 — Distribution cleanup
- Delete `sdd-ext/engine/` (Go), `sdd-ext/hooks/sdd.json`, the crosscompile path.
- Embed or ship the skills via a slim installer (or bundle them in-binary).
- One binary: `grok`. Update `README.md` / `install.sh`.

**Deliverable:** friends install a single `grok` binary; no Go, no separate hooks.

---

## Recon spikes needed before coding

Two insertion points need a short code read before their phase:
- **Phase 3:** exact contributor seam in `stop_gate.rs` for native additionalContext
  + where to store the per-session "engaged" flag.
- **Phase 5:** exactly where `max_turns` is resolved per turn in `agent_ops.rs` and
  whether it can be varied per-turn (vs. per-session).

## Open risks
- Divergence is now permanent: pulling upstream grok-build fixes becomes manual
  cherry-pick. Decide a cadence (e.g. review upstream quarterly).
- Registering a native tool touches a synced file (`registry/types.rs`) — fine
  now that divergence is accepted, but it's the highest-churn upstream file.
