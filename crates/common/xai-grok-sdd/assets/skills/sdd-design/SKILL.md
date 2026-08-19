---
name: sdd-design
description: Use when a feature task ships user-facing UI and has no approved design yet — gather the user's visual references, sketch the layout as an ASCII wireframe in the design doc, stop at the approval gate, then code the screen hi-fi directly (no placeholder build).
---

# Design (UI tasks only)

A UI task needs an **approved design before any UI code**. Not a UI task? Skip this phase.

The design is a lightweight, fast-to-review spec — the user's references plus an ASCII wireframe — **not a placeholder build**. Product screens are coded hi-fi directly, so the human approves the spec (catching a bad layout before any code exists), then the screen is coded once, for real.

## Base vs product

- **Base components** (Button, Input, Card, tokens, FormField/Modal) live in the `/design-system` workbench, built once via `sdd-design-system`. This phase doesn't rebuild them.
- **Product screens** (a sign-in screen, a cash-register view) are coded directly in the app, composed from the base. This phase designs *those* — as a wireframe.

Dividing test: *business-agnostic and reusable?* → base (workbench). *Feature-specific?* → product (design here, code directly).

## How to run it

1. **Gather references first — ask the user** (plain reply, not a command):
   > ¿Tenés una referencia para esta pantalla? Figma, una imagen o boceto, un sitio a imitar, o me lo describís?
   - **Figma URL** → use the figma MCP for frames, tokens, layout.
   - **Image / screenshot** → save under `sdd/designs/<NNN>/`.
   - **Site to emulate** → note the URL and what to borrow.
   - **Only a description** → capture it in words.
   Record it in the doc's `## References`. If they have none, describe the intended look yourself for them to confirm.

2. **Sketch an ASCII wireframe** of each screen and key state (default, empty, loading, error) — regions and where each component sits. This is what the human reviews: distribution and hierarchy, not pixels.

3. **Record the composition** — the workbench components it builds from, the new product components you'll code, and any missing base primitive that must be added to the workbench first (never hand-roll a missing primitive in the screen).

4. **Record + open the gate:** `sdd` tool, `action: "design"`, `args: ["decisions/NNN-name.md", "<screen or flow>"]`. Fill `## References`, `## Wireframe`, `## Composition`.

5. **Stop at the gate.** The human checks the wireframe + references. Ask for a short approval ("aprobado") — don't make them type a command; on approval call `action: "approve-design"`, `args: ["designs/NNN-slug"]` yourself. Without an approved design the loop refuses UI code.

## After approval

Load `sdd-implement` and code the screen **hi-fi, directly**, composing from the approved base components and following the wireframe.

## Verification split

- **Layout/visual** — the human verifies on **localhost** (manual QA). No screenshots committed.
- **Logic behind the UI** (formatting, state, calculations) — code tests via `sdd-test`.

## Anti-patterns

- ❌ Building a placeholder composition before coding the real screen.
- ❌ Sketching without asking for references first (you'll guess the look wrong).
- ❌ UI code before the wireframe is approved.
- ❌ Raw CSS where a base component exists — compose from the workbench.
