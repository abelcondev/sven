# Spec-Driven Development (this project uses the SDD loop)

This repository is driven by a single Spec-Driven Development loop. The full,
durable state lives on disk under `sdd/` — read it, don't rely on memory.

## Read this first, every session

- Read `sdd/index.md` first — it is the map of the knowledge base (the loop, the
  branch/PR policy, UI conventions, test conventions, and the list of decisions).
- To find out what to do next, run `grok-sdd next`. It reads disk + the current
  git branch and prints the single recommended step. It is cheap and resumable —
  prefer it over guessing where you are. `grok-sdd status` gives the fuller picture.
- Do **one** step, then stop. Never batch the whole loop in a turn.

## The loop

```
propose → approve → [design → approve-design, for UI] → task → branch →
implement (TDD) → review → ship → merge → cleanup → repeat
```

Everything is a pass through this same loop: discovery, the stack decision, the
design-system foundations, and every feature. There are no separate flows.

Each phase has a dense skill. When `grok-sdd next` names a `skill:`, load that
skill first (it carries the phase's real rules), then act. The phase skills are:
`sdd-discovery`, `sdd-stack`, `sdd-design-system`, `sdd-design`, `sdd-task`,
`sdd-test`, `sdd-implement`, `sdd-review`, `sdd-ship`.

## Human gates — stop and hand control back

Two steps are human gates. At a gate: do **not** proceed. Point the user at the
file, summarize it, and ask for approval in plain language ("¿lo apruebo?"). Do
**not** make them type a command — when they approve, run it for them.

- **Proposal approval** — after `grok-sdd propose`, the user reviews
  `sdd/proposal.md`. On approval you run `grok-sdd approve`.
- **Design review** — a UI task needs an approved design before any UI code. The
  user reviews the design spec; on approval you run `grok-sdd approve-design`.

## Branch & PR policy

- The default branch (`main`/`master`) is protected. A compiled branch guard
  refuses code writes there — branch once per proposal before editing code.
- One proposal → one branch → one PR. All of a proposal's tasks share its branch.
  Open the PR as a **draft** from the first task; mark it ready (`gh pr ready`)
  only when every task in the proposal is done.
- Push with `git push origin HEAD` (no `-u` — it writes `.git/config`, which the
  sandbox refuses). Never merge or push to the protected branch yourself.
- After a proposal's PR merges, run `grok-sdd cleanup` to delete the merged branch.

## Implementation route — pick the smallest useful one

Size, file count, and perceived risk **never** select SDD on their own. Only an
explicit human request or an already-approved proposal puts you in the SDD loop.

- **Direct (default):** 1–3 files of understood work — edit inline, no ceremony.
- **Delegated:** 4+ files, broad exploration, or adversarial test/build/review —
  hand a narrow slice to a fresh-context subagent. This creates **no** SDD state.
- **SDD:** only after an explicit user request or an accepted proposal.

While a proposal is active, continue its tasks under the loop (`grok-sdd next`);
do not start parallel ad-hoc work. Never run `grok-sdd propose` just because a
change looks big.

## UI work

- Base UI (tokens, primitives, generic business-agnostic patterns) is built once
  in an isolated `/design-system` workbench route, reviewed live. Product screens
  are coded hi-fi directly, composing from the workbench — never hand-rolled.
- A UI task must have an **approved design** before any UI code. Look & states are
  verified live by the human (localhost / workbench); logic (formatting,
  validation, calculations) is verified by code tests via `sdd-test`.

## Closing a task

- Run review (`sdd-review`) before the PR; every high-severity finding must be
  fixed. Residual medium/low findings you consciously accept become tracked
  follow-up tasks: `grok-sdd ship <task> --residual "<finding>"` (repeatable).
- Close with `grok-sdd ship <task>` (it pre-flights branch/remote/gh, then closes).

Everything in `sdd/` is written in English regardless of the conversation language.
Always end a turn by naming the next step: "✅ Done: … ▶ Next: … — shall I?"
