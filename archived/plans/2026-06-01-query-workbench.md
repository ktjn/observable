# Query Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a shareable `/workbench` notebook for ad-hoc exploration with three fixed starter blocks, URL-backed state, and per-block NLQ/raw editing across metrics, logs, and traces.

**Architecture:** The workbench is a frontend-only composition layer over the existing NLQ pipeline. A route-local URL codec stores notebook state, the shell owns shared context and layout, and each block owns its query mode, draft text, execution state, and result rendering. The first slice keeps the layout fixed and the state serializable so later reordering, add/remove, or persistence can be added without changing the base model.

**Tech Stack:** React 19, TanStack Router, TanStack Query, Vite, Tailwind CSS v4, Monaco editor, Vitest, Playwright + axe.

---

### Task 1: Add the workbench state codec and route contract

**Files:**
- Create: `apps/frontend/src/features/workbench/workbenchState.ts`
- Create: `apps/frontend/src/features/workbench/workbenchState.test.ts`
- Modify: `apps/frontend/src/router.ts`
- Modify: `apps/frontend/src/pages/NlqQueryPage.tsx`

- [ ] **Step 1: Write the failing codec tests**

Create tests that prove the notebook state codec is deterministic and resilient:
- the starter notebook encodes and decodes round-trip with `version: 1`
- block order is preserved in the encoded blob
- invalid JSON falls back to the starter notebook state
- unknown versions are rejected rather than partially applied
- `/nlq` can continue to render the new workbench surface as a compatibility alias

- [ ] **Step 2: Run the focused test file and confirm it fails**

Run:
```bash
cd apps/frontend
npm test -- --run src/features/workbench/workbenchState.test.ts
```

Expected: the new test file does not exist yet, so Vitest fails with a missing-file or missing-export error.

- [ ] **Step 3: Implement the codec and route shape**

Implement a versioned `NotebookStateV1` model with a compact URL-safe serializer and parser. Keep the state shape small enough to fit in one query param and make the starter layout explicit:
- title
- fixed block list
- block signal target
- block mode
- block draft
- collapsed state

Add the new `/workbench` route in `apps/frontend/src/router.ts` and keep `/nlq` as a compatibility route for this slice so existing links still land on the new notebook.

- [ ] **Step 4: Re-run the focused test file**

Run:
```bash
cd apps/frontend
npm test -- --run src/features/workbench/workbenchState.test.ts
```

Expected: pass.

- [ ] **Step 5: Commit the codec slice**

Commit the route contract and codec changes before moving on so the URL model is isolated from the notebook UI work.

---

### Task 2: Build the notebook shell and fixed three-block layout

**Files:**
- Create: `apps/frontend/src/pages/QueryWorkbenchPage.tsx`
- Create: `apps/frontend/src/features/workbench/QueryWorkbench.tsx`
- Create: `apps/frontend/src/features/workbench/NotebookBlock.tsx`
- Create: `apps/frontend/src/features/workbench/NotebookResults.tsx`
- Create: `apps/frontend/src/features/workbench/QueryWorkbench.test.tsx`
- Modify: `apps/frontend/src/router.ts` if the route is not already wired in Task 1
- Modify: `apps/frontend/src/components/AppShell.tsx` if the nav needs a primary workbench entry

- [ ] **Step 1: Write the failing notebook-shell tests**

Create RTL tests that cover:
- the page renders a notebook title and a fixed three-block starter layout
- the three blocks are labeled metrics, logs, and traces
- each block has its own run control and loading state
- the blocks use the existing tenant context and global date range when submitting queries
- a successful frame renders through `VisualizationPanel`
- decline/error/invalid-response states are shown per block, not globally

- [ ] **Step 2: Run the focused notebook test and confirm it fails**

Run:
```bash
cd apps/frontend
npm test -- --run src/features/workbench/QueryWorkbench.test.tsx
```

Expected: fail because the notebook shell does not exist yet.

- [ ] **Step 3: Implement the shell and per-block execution**

Build the workbench shell around the existing NLQ API:
- use `useTenantContext()` for the tenant ID
- use `useGlobalDateRange()` for the shared time range
- use `submitNlqQuery()` for execution
- render `VisualizationPanel` for frame results
- keep loading/error state inside each block
- preserve each block’s result independently so one query does not overwrite another

Keep the starter layout fixed at three blocks. Do not add drag-and-drop or block reordering in this slice.

- [ ] **Step 4: Re-run the notebook test**

Run:
```bash
cd apps/frontend
npm test -- --run src/features/workbench/QueryWorkbench.test.tsx
```

Expected: pass.

- [ ] **Step 5: Commit the notebook shell**

Commit the shell and block rendering work separately from the editor work so the layout can be reviewed independently.

---

### Task 3: Add Monaco-backed editing and the NLQ/raw mode toggle

**Files:**
- Create: `apps/frontend/src/features/workbench/NotebookEditor.tsx`
- Create: `apps/frontend/src/features/workbench/NotebookEditor.test.tsx`
- Modify: `apps/frontend/package.json`
- Modify: `apps/frontend/package-lock.json`
- Modify: `apps/frontend/vite.config.ts` if Monaco worker setup is required

- [ ] **Step 1: Write the failing editor-mode tests**

Create tests that prove:
- each block opens in NLQ mode by default
- switching a block to raw mode preserves the draft text
- raw mode renders and validates JSON editing for the IR payload
- invalid raw JSON blocks submission and shows a block-level validation message
- switching back to NLQ mode does not clear the draft

- [ ] **Step 2: Run the editor tests and confirm they fail**

Run:
```bash
cd apps/frontend
npm test -- --run src/features/workbench/NotebookEditor.test.tsx
```

Expected: fail until Monaco and the toggle logic exist.

- [ ] **Step 3: Add the Monaco dependency and editor wrapper**

Add the Monaco editor dependency with npm, then wire the workbench editor component so:
- NLQ mode is the default text entry path
- raw mode uses JSON syntax highlighting
- the editor content is owned by the block state
- the editor surfaces the current mode clearly to the user

If Monaco worker loading needs Vite configuration, keep the config change scoped to the frontend package and document the exact plugin choice in the PR.

- [ ] **Step 4: Re-run the editor tests**

Run:
```bash
cd apps/frontend
npm test -- --run src/features/workbench/NotebookEditor.test.tsx
```

Expected: pass.

- [ ] **Step 5: Commit the editor slice**

Commit the editor-mode work after the mode toggle and raw JSON validation are both covered by tests.

---

### Task 4: Move the primary entry points to `/workbench` and keep `/nlq` as a compatibility alias

**Files:**
- Modify: `apps/frontend/src/components/AppShell.tsx`
- Modify: `apps/frontend/src/pages/HomePage.tsx`
- Modify: `apps/frontend/src/pages/NlqQueryPage.tsx`
- Modify: `apps/frontend/src/router.ts` if the alias route needs a redirect or shared component
- Create: `apps/frontend/src/pages/HomePage.test.tsx`
- Create: `apps/frontend/src/components/AppShell.test.tsx`

- [ ] **Step 1: Write the failing navigation tests**

Add tests that prove:
- the sidebar and home quick-nav point at `/workbench` as the primary surface
- visiting `/nlq` still reaches the workbench notebook for older links
- the visible labels are consistent with the new notebook name

- [ ] **Step 2: Run the navigation tests and confirm they fail**

Run:
```bash
cd apps/frontend
npm test -- --run src/pages/HomePage.test.tsx src/components/AppShell.test.tsx
```

Expected: fail until the route links are updated.

- [ ] **Step 3: Update the visible entry points**

Point the visible navigation at `/workbench`, keep `/nlq` as an alias for the first slice, and make sure the home page and app shell use the same name for the notebook surface. Do not remove the old route until the new route is stable in the UI and tests.

- [ ] **Step 4: Re-run the navigation tests**

Run:
```bash
cd apps/frontend
npm test -- --run src/pages/HomePage.test.tsx src/components/AppShell.test.tsx
```

Expected: pass.

- [ ] **Step 5: Commit the entry-point update**

Commit the visible navigation update separately so reviewers can verify the route migration without reading the notebook implementation at the same time.

---

### Task 5: Add the workbench accessibility smoke test

**Files:**
- Create: `apps/frontend/e2e/workbench.spec.ts`
- Modify: `apps/frontend/e2e/accessibility.spec.ts` if the existing a11y suite is the better home for the new case

- [ ] **Step 1: Write the failing Playwright axe test**

Add a smoke test that:
- authenticates with the existing mocked tenant/session pattern
- stubs the NLQ responses needed for the three starter blocks
- opens `/workbench`
- waits for the notebook shell to render
- asserts zero axe violations on the new page

- [ ] **Step 2: Run the accessibility test and confirm it fails**

Run:
```bash
cd apps/frontend
npx playwright test e2e/workbench.spec.ts
```

Expected: fail until the page exists and the selectors are stable.

- [ ] **Step 3: Implement the test fixtures and page checks**

Reuse the existing Playwright auth and NLQ mocking patterns from the signal explorer specs. Keep the test focused on the new page shell rather than duplicating every block interaction in e2e.

- [ ] **Step 4: Re-run the accessibility test**

Run:
```bash
cd apps/frontend
npx playwright test e2e/workbench.spec.ts
```

Expected: pass with no axe violations.

- [ ] **Step 5: Commit the a11y coverage**

Commit the accessibility smoke test after the shell is stable and keyboard/heading structure is validated.

---

## Self-Review

### Spec coverage
- URL-backed notebook state: Task 1
- fixed starter layout with three blocks: Task 2
- all three signals: Task 2
- NLQ/raw hybrid editing: Task 3
- Monaco requirement: Task 3
- `/workbench` route and shareable entry points: Tasks 1 and 4
- compatibility for existing `/nlq` entry points: Tasks 1 and 4
- frame rendering and provenance reuse: Task 2
- accessibility coverage: Task 5

### Placeholder scan
- No `TBD`, `TODO`, or vague "handle edge cases" steps remain.
- File paths are concrete.
- Commands are explicit.

### Type and boundary consistency
- `NotebookStateV1` is the single state model the rest of the tasks build on.
- `QueryWorkbench` is the shell component name used consistently across page, tests, and route wiring.
- The alias route stays in place until the new primary route is fully exercised.

## Execution order
1. Complete Task 1.
2. Complete Task 2.
3. Complete Task 3.
4. Complete Task 4.
5. Complete Task 5.
