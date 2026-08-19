---
name: sdd-implement
description: Use when implementing a pending SDD task — build it on the proposal's feature branch scaled to the task's tier (the `sdd` tool prints the `plan (...)` line), then carry it through review to a PR. One PR per proposal.
---

# Implement

Build the task, then carry it through review to a PR. **The `sdd` tool's `plan (...)` line is authoritative** — it scales the loop to the task's tier. Don't add ceremony it didn't ask for.

## Order of operations

1. **Stay on the proposal's branch.** `propose` already opened it (`sdd/prop-<slug>`); all its tasks share one branch, one PR. If HEAD somehow sits on `main`/`master` (a compiled guard refuses code writes there), return to it — `git branch --list 'sdd/prop-*'` — or fall back to `git checkout -b feat/<decision-slug>`.

2. **Write the code, scaled to tier.**
   - Plan says **TDD** (standard/critical): failing test first → minimal code to pass → refactor. Translate each **Given/When/Then** into a test (see `sdd-test`).
   - Plan says **no test-first** (trivial): compose the screen/change from existing base components and code it directly. Still cover any real logic (a calculation, a reducer) with a plain test — just not test-first, and don't test a pure composition.
   - Either way: smallest diff that fully solves the task, in the surrounding style. No speculative abstraction, no unrelated refactors, no dependency churn.

3. **Validate the change — but don't full-build in the loop.** Run the fast validators scoped to the diff (**typecheck + lint + the scoped tests**) as recorded in `sdd/index.md`. **The full production build runs once, at ship — never after each fix** (it's the slowest check and re-running it per edit is where turns are lost). Never proceed while the fast validators fail.

4. **Review** — load `sdd-review` and review at the task's tier (the plan says how deep). Fix every high-severity finding before closing.

5. **Close + ship** — load `sdd-ship`: close the task via the `sdd` tool (`action: "done"`, `args: ["<task-name>"]`), then run the ship steps. A **trivial** task does steps 2–5 in **one turn** (its plan says "all in this one turn"); standard/critical stop after each phase.

## Working context (`sdd/context.md`)

Before implementing, read `sdd/context.md` — the proposal's already-discovered surface (API shapes, store/module methods, key paths, gotchas) so you don't re-explore what a previous turn already mapped. As you find a stable fact worth the *next* turn not re-deriving, record it there under this proposal's heading — a short map, not a log. **This is what survives context compaction: lean on it instead of re-reading the backend, and update it before a long step so a compaction mid-task doesn't force a full re-orientation.** Clear the proposal's section when its PR merges.

## Delegating to fresh-context subagents (optional)

If your harness can spawn subagents (grok's `/workflow`), hand a phase to a fresh-context agent where fresh context or parallelism pays: **coding** a fully-specified task, **exploration** (read-only), **planning**, or **review** (for independence). Each starts with zero context and grounds itself in the on-disk `sdd/` artifacts — brief it with the task/decision ref, not pasted context. It inherits your model, sandbox, and permissions — never more. No subagents? Do the phases inline; the loop doesn't depend on delegation.

## Definition of done

- Every `code`-level Given/When/Then the plan required has a passing test.
- Fast validators pass (full build verified at ship).
- `sdd-review` run at tier; every high-severity finding fixed, residual medium/low recorded via `done` with `residual: ["…"]` and noted in the PR.
- Branch pushed, PR prepared — never merged to the protected branch by you.

## Hand-off

Close every turn with the tool's `▶ Next step` block, surfaced short:

> ✅ Hecho: <what you finished>.
> ▶ Sigue: <next step> — ¿lo hago?

At a human gate, say so and wait. Otherwise offer to proceed.

## Anti-patterns

- ❌ Writing code on `main`.
- ❌ Running the full production build after every fix (build at ship, not in the loop).
- ❌ TDD ceremony or a craft subagent on a trivial composition the plan didn't ask for.
- ❌ Marking done with failing validators or unaddressed high-severity findings.
