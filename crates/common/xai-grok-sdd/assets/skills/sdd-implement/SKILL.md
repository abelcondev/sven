---
name: sdd-implement
description: Use when implementing a pending SDD task — TDD red→green→refactor on a feature branch, honoring the task's Given/When/Then, then chain to sdd-test, sdd-review, and sdd-ship before closing. One PR per proposal.
---

# Implement

Build the task with test-driven discipline, on a feature branch, and carry it all the way through review to a PR. This skill orchestrates the tail of the loop.

## Order of operations

1. **Stay on the proposal's branch.** The `sdd` tool's `propose` action already opened this proposal's branch (`sdd/prop-<slug>`); all of its tasks share it — one branch, one PR. You should already be on it, so implement here. Only if HEAD somehow sits on `main`/`master` (a compiled guard refuses code writes there), return to the proposal's branch — check `git branch --list 'sdd/prop-*'` — or, as a fallback, `git checkout -b feat/<decision-slug>`.

2. **TDD: red → green → refactor.** Write the failing test first, then the minimal code to pass, then refactor. Translate each **Given/When/Then** acceptance criterion into a test. For the test conventions (only `.test.ts` code tests, dedicated folder, what the agent writes vs. what the human verifies manually), load **`sdd-test`**.

3. **Match the codebase.** Smallest diff that fully solves the task, in the surrounding style. No speculative abstraction, no unrelated refactors, no dependency churn the task did not require.

4. **Run the validators** scoped to the change — tests, typecheck, lint, build — as recorded in `sdd/index.md`. Never proceed while they fail.

5. **Review before you ship.** Once green, load **`sdd-review`**: a fresh-context pass over the diff across two lenses — correctness + security, and craft/maintainability (a reviewer subagent that flags oversized files, poor wiring, leaked coupling, missed reuse). Remediate the findings in one pass, then re-review. Every high-severity finding must be fixed before closing.

6. **Close and open the PR.** Load **`sdd-ship`** for the pre-flight + close + PR steps. Close the task by calling the `sdd` tool with `action: "done"` and `args: ["<task-name>"]`, then run the ship steps yourself via git/gh (see `sdd-ship`).

## Working context (`sdd/context.md`)

Before implementing, read `sdd/context.md` — it holds the proposal's already-discovered surface (API shapes, store/module methods, key file paths, gotchas) so you don't re-explore the same files a previous turn already mapped. As you discover a stable fact worth the *next* turn not re-deriving, record it there under this proposal's branch heading — keep it a short map, not a log. This is what survives context compaction; lean on it instead of re-reading the backend each turn. When the proposal's PR merges, clear its section.

## Delegating to fresh-context subagents (optional)

If your harness can spawn subagents (e.g. grok's `/workflow`), you can hand a phase to a fresh-context agent — smallest useful route, so delegate only where fresh context or parallelism pays:

- **coding a task** — a single, fully-specified task (its Given/When/Then are written). Spawn one per independent task to build them in parallel, briefing each with only the task ref.
- **exploration** — read-heavy investigation before implementing (where does X live, how is Y wired). Read-only, returns a conclusion.
- **planning** — drafting a proposal/decision or breaking work into tasks (see `sdd-discovery`, `sdd-stack`, `sdd-task`).
- **review** — at the review gate, for independence (see `sdd-review`).

Each subagent starts with zero context and grounds itself in the on-disk `sdd/` artifacts, so brief it with the task/decision ref rather than pasting context. It inherits your model, sandbox, and permission mode — never more authority. If your harness has no subagents, just do these phases inline; the loop does not depend on delegation.

## Definition of done

- Every `code`-level Given/When/Then has a passing test.
- Validators pass.
- `sdd-review` run (both lenses); every high-severity finding fixed, residual medium/low recorded as a follow-up task via the `sdd` tool with `action: "done"` and `residual: ["…"]` (and noted in the PR).
- Branch pushed, PR prepared — never merged to the protected branch by you.

## Always end pointing to the next step

Never end a turn with the user unsure what happens next. The `sdd` tool's `done`, `next`, and `status` actions print a `▶ Next step` block (with a `then:` horizon) — surface it. Close every turn with a short, consistent hand-off:

> ✅ Hecho: <what you just finished>.
> ▶ Sigue: <the next step's summary> — ¿lo hago?

If it's a human gate (approval), say so and wait. If it's automated work, offer to proceed. The user should always see the immediate next step and where it leads without having to ask.

## Anti-patterns

- ❌ Writing code on `main`.
- ❌ Implementation before the test (that is not TDD).
- ❌ Marking the task done with failing validators or unaddressed review findings.
