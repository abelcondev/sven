---
name: sdd-test
description: Use while implementing a task to write its tests — code tests only (e.g. .test.ts), one per Given/When/Then, in the project's dedicated test folder. Browser/visual/e2e is manual human QA, never auto-generated.
---

# Test

Cover the task's acceptance criteria with **code tests only**. On standard/critical tasks this is TDD (tests first, inside implementation). On a **trivial** task the plan skips test-first — cover any real logic with a plain test, but don't test a pure composition of existing components.

## Scope: code tests, not browser tests

- ✅ **Code tests** (unit/logic) in the project's runner — e.g. `*.test.ts` with Vitest, `*_test.go` with `go test`. Cover calculations, reducers, validators, stores, formatting, edge cases.
- ❌ **Browser / visual / e2e** (Playwright, "looks right on the tablet") — **manual QA owned by the human.** Don't auto-generate these; criteria marked `manual` are theirs to verify.

## Not Cucumber

The Given/When/Then is a **prose spec**, not an executable `.feature` file — no step definitions, no BDD framework. Translate each criterion into an ordinary test:

```
Given a comanda with 2 pollos at S/30   → arrange: build the comanda
When the cashier charges with S/100      → act:    charge(comanda, 100)
Then the change due is S/40              → assert: expect(change).toBe(40)
```

## Conventions come from sdd-stack (`sdd/index.md`)

- **Dedicated test folder mirroring the source tree** (e.g. `src/caja/vuelto.ts` → `tests/caja/vuelto.test.ts`), unless the project chose colocated. Keep tests where that convention says.
- **Import via the recorded path alias, not deep relative paths** — `import { charge } from '$lib/caja/vuelto'`, never `../../../src/lib/caja/vuelto`. The alias is stable no matter where the test sits; if none is recorded, match the existing sibling tests.

## Coverage bar

- Every `code`-level Given/When/Then → at least one passing test, plus the edge cases the criteria imply (empty, boundary, error).
- Don't pad with trivial tests to inflate a number; cover behavior.

## Anti-patterns

- ❌ `.feature` files / Cucumber / step definitions.
- ❌ Auto-writing Playwright/e2e for the visual layer.
- ❌ Testing a pure UI composition that has no logic of its own.
- ❌ Scattering tests against the project's chosen layout.
