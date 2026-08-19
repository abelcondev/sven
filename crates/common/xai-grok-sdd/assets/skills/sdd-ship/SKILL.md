---
name: sdd-ship
description: Use to close a reviewed task and land it in the proposal's pull request — run the full build once here, mark the task done, push the branch, keep a draft PR open until the proposal is complete. One PR per proposal; never merge to the protected branch yourself.
---

# Ship

Close the task and keep the proposal's PR current — the end of one turn of the feature loop. The PR is a **draft** until the whole proposal is done, so CI and reviewers can follow along without anyone being pinged to merge a half-built proposal.

## Preconditions

- Fast validators pass (typecheck, lint, scoped tests).
- **The full production build passes — run it once, here.** This is the single build in the loop: the implement/review passes deliberately skip it to save turns, so ship is where it's verified. If it fails, fix and re-run before closing.
- `sdd-review` run at tier; every high-severity finding fixed, residual medium/low listed in the PR.

## How to run it — resumable

A previous turn may have run out of budget partway through. Check what's already true; don't redo finished work or open a second PR. Front-load the close and push (steps 1, 3) before any polish, so a cutoff never leaves work unclosed or unpushed.

1. **Pre-flight, then close.** Verify you're on the proposal's feature branch (not `main`/`master`), `origin` is set and reachable (`git remote get-url origin` + `git ls-remote origin HEAD`), and `gh auth status` is authenticated. **These checks are stable within a session — run them once on the first ship, then skip on later tasks of the same proposal unless a push fails.** Then close: `sdd` tool, `action: "done"`, `args: ["<task-name>"]`, passing each accepted residual as `residual: ["…"]`. Its output prints the proposal's task checklist and whether the proposal is complete — use that verbatim for steps 5–6. (`done` is idempotent.)

2. **One PR per proposal.** All tasks share one branch and one PR. Never open a PR per task.

3. **Push** (no `-u` — it writes `.git/config`, which the sandbox refuses):
   ```
   git push origin HEAD
   ```

4. **Open the draft PR, or reuse the existing one:**
   ```
   gh pr view --json number >/dev/null 2>&1 || gh pr create --fill --draft
   ```
   Never push to or merge the protected branch yourself.

5. **Refresh the PR body** with the current checklist from step 1 plus the `manual`-level acceptance criteria (browser/visual/e2e) the human still verifies:
   ```
   gh pr edit --body "<checklist + manual QA notes>"
   ```

6. **Mark ready only when the proposal is complete** (step 1 reported no pending tasks). Until then leave it a draft and move to the next task:
   ```
   gh pr ready
   ```

## After merge

Prune the merged branch (`git branch -D <branch>`), then call the `sdd` tool `action: "next"` to get back to the top of the loop. Relay its `▶ Next step` block as a short hand-off — `✅ Hecho: … · ▶ Sigue: … — ¿lo hago?`.

## Anti-patterns

- ❌ Merging to `main` yourself, or a separate PR per task.
- ❌ Marking the PR ready while tasks of the proposal are still pending.
- ❌ Re-running the pre-flight on every task of the same proposal — once per session is enough.
- ❌ `git push -u …` — the `-u` fails in the sandbox.
- ❌ Closing without listing what the human still verifies manually.
