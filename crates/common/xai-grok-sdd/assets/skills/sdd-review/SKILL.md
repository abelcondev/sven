---
name: sdd-review
description: Use after a task is green and before its PR — review the diff scaled to the task's tier (the `sdd` tool's `plan (...)` line says how deep), auto-remediate in one pass, then re-review if round 1 changed code. High-severity findings block done/PR. Precedes (does not replace) the human's PR review.
---

# Review (automated quality gate)

A second pass with clean context catches blind spots the implementing context misses. Runs **before the PR**; the human still reviews at merge.

## Scale to the task's tier — follow the plan

The `sdd` tool's `plan (...)` line already resolved the depth. Obey it:

- **trivial** — Lens 0 (cheap greps) + a single correctness pass. No craft subagent, no round 2. **Don't refactor adjacent files** — fix only what the task touched.
- **standard** — Lens 0 + Lens A + Lens B, with the remediation loop; round 2 only if round 1 changed code.
- **critical** — the full thing: both rounds **always**, security lens mandatory.

If a trivial/standard task turns out to touch money, auth, or data, **bump it up** and say so. Never silently under-review; only escalate.

## Lens 0 — cheap static pre-pass (run first)

Run the deterministic checks under `## Cheap review checks` in `sdd/index.md` against the diff — plain greps (literal glyphs in UI, a domain type redefined outside its home, new `TODO`/`FIXME`, etc.). Fix every hit now, cheaply, so the model-driven lenses stay focused on real judgment. If `sdd/index.md` records none, skip (and consider adding the obvious ones).

## Lens A — correctness & security

Audit `git diff` against the base on two axes:

- **Correctness** — bugs, wrong logic, unhandled errors, missing edge cases, reuse/simplification misses, checked against the Given/When/Then.
- **Security** — secrets, injection, authz gaps, unsafe I/O. Mandatory for `critical`.

## Lens B — craft & maintainability (fresh context; standard/critical only)

Judge the diff with **fresh eyes** — ideally a subagent briefed with the task ref + diff (it didn't write the code, so it won't rationalize it). It judges *how* the feature was built, returning `{ file, lines, category, severity, why, concrete_fix }`:

- **Size** — a file past ~300–400 lines or an over-long function/component: flag as a split candidate (a *smell*, say *why* splitting helps — not an automatic fail).
- **Wiring** — clean imports, layering respected; UI screens compose from `/design-system` (not hand-rolled), presentation/behavior split holds.
- **Reuse** — duplicated logic that should be shared; hand-rolled UI where a workbench component exists.
- **Cohesion & conventions** — one responsibility, clear names, no dead code/TODOs; matches `sdd/index.md`.

Severity: **high** = broken wiring, layering violation, unmanageable file, or duplication that will bite → **blocks done/PR**. **medium** = a clear improvement → fixed this pass. **low** = a nit → fixed if cheap, else noted.

## Remediation loop

1. Lens 0 → fix; then Lens A (+ Lens B for standard/critical); collect findings.
2. Fix every **high** and **medium** — the cause, not the symptom. Keep the diff scoped to the task.
3. Re-run the **fast** validators (typecheck, lint, scoped tests). The full build is ship's job, not the review loop's.
4. **Round 2 only if round 1 changed code** (always for `critical`). If nothing changed, the diff is unchanged — skip it.
5. Stop after **2 rounds** — don't chase polish.

## Gate

- **Don't call `done` with any unresolved high-severity finding.** If you judge one a false positive, say why.
- Any **residual medium/low** you consciously skip becomes tracked work: pass it to `done` as `residual: ["<finding>"]` (repeatable) so it lands as a follow-up task, and note it in the PR. Nothing hidden, nothing forgotten.

## Anti-patterns

- ❌ Going straight from green to `done` with no review.
- ❌ A craft subagent or round 2 on a trivial task the plan didn't ask for.
- ❌ Full-building in the review loop (that's ship).
- ❌ Reviewing craft in the implementing context (it rationalizes its own choices).
- ❌ Expanding scope under the banner of "review" — that's a new task.
