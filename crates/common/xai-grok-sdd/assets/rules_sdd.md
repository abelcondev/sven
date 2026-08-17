# Spec-Driven Development (opt-in loop)

This project can run a Spec-Driven Development (SDD) loop, but it is **opt-in**.
You enter it only on an explicit request from the user, or while a proposal is
already active — never because a change "looks big". A greeting, a question, or a
small understood fix is **not** a request to run the loop: answer it directly and
stop. Size, file count, and risk never select the loop on their own.

## Using the loop (when engaged)

The durable state lives on disk under `sdd/` — read it, don't rely on memory.

- To find where you are, call the `sdd` tool: action `next` gives the single
  recommended step; action `status` gives the fuller picture. It is cheap and
  resumable — prefer it over guessing.
- Do **one** step, then stop. Never batch the whole loop in a turn. The tool's
  output names the next step and the phase skill to load first (`sdd-discovery`,
  `sdd-stack`, `sdd-design-system`, `sdd-design`, `sdd-task`, `sdd-test`,
  `sdd-implement`, `sdd-review`, `sdd-ship`) — load it, then act.

## Human gates — stop and hand control back

Two steps are human gates. At a gate: do **not** proceed. Point the user at the
file, summarize it, and ask for approval in plain language ("¿lo apruebo?"). Don't
make them type anything — when they approve, run the tool for them.

- **Proposal approval** — after `propose`, the user reviews `sdd/proposal.md`. On
  approval, call the `sdd` tool (action `approve`).
- **Design review** — a UI task needs an approved design before any UI code. On
  approval, call the `sdd` tool (action `approve-design`).

## Branch & PR policy

The default branch (`main`/`master`) is protected. `propose` opens the proposal's
feature branch; all of its tasks share it — one proposal → one branch → one draft
PR, marked ready only when every task is done. Push with `git push origin HEAD`.
Never merge or push to the protected branch yourself.

Everything under `sdd/` is written in English regardless of the conversation language.
