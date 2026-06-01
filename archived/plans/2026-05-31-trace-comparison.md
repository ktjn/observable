# Trace Comparison Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a trace comparison workflow that lets an operator open a dedicated compare view for two traces, inspect them side by side, and spot the main delta in duration, status, service path, and span count without leaving the trace surface.

**Status:** Completed 2026-05-31.

**Architecture:** Keep the feature frontend-only and reuse the existing trace API. Add a compare route under `/traces/compare` that accepts two trace IDs as route search params, fetches both traces with the existing `getTrace()` API, and renders a two-column comparison view plus a compact diff summary. Add a compare entry point from trace detail so the current trace can be sent to the compare view quickly.

**Tech Stack:** TypeScript, React, TanStack Router v1, TanStack Query, Vitest + `@testing-library/react`.

**Source spec:** `spec/05-frontend.md §9.3` and §9.4

---

## Files Changed

| File | Change |
|---|---|
| `apps/frontend/src/pages/TraceComparePage.tsx` | New: route loader + compare form/view |
| `apps/frontend/src/pages/TraceCompare.tsx` | New: reusable compare UI + diff helpers |
| `apps/frontend/src/pages/TraceCompare.test.tsx` | New: compare page/component tests |
| `apps/frontend/src/pages/TraceDetail.tsx` | Add compare entry point from the current trace |
| `apps/frontend/src/pages/TraceDetail.test.tsx` | Assert the compare entry point is present and targets the compare route |
| `apps/frontend/src/router.ts` | Register `/traces/compare` before the dynamic trace-detail route |
| `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` | Mark Trace Comparison complete and record the next slice |
| `docs/agent-context.md` | Update the active roadmap note after completion |

---

## Task 1: Add the compare route and page shell

**Files:**
- Create `apps/frontend/src/pages/TraceComparePage.tsx`
- Create `apps/frontend/src/pages/TraceCompare.tsx`
- Modify `apps/frontend/src/router.ts`

- [ ] Add a static `/traces/compare` route with route-local search params for `left` and `right`.
- [ ] Render a small compare form when either trace ID is missing.
- [ ] Fetch both traces with the existing `getTrace(tenantId, traceId)` API when both IDs are present.
- [ ] Render a loading and empty state that match the existing explorer style.

**Acceptance target:** opening `/traces/compare?left=<trace-a>&right=<trace-b>` shows both traces without navigating away from the trace surface.

**Verification:** run the frontend compare-page test file, then a frontend build.

**Rollback path:** remove the route and compare page files; the existing trace detail and trace explorer remain unchanged.

---

## Task 2: Render a side-by-side comparison

**Files:**
- `apps/frontend/src/pages/TraceCompare.tsx`

- [ ] Show the two traces in adjacent panels with the same trace summary fields.
- [ ] Add a compact diff summary for span count, total duration, root service, and status.
- [ ] Include a short path-diff summary derived from the root span service/operation sequence.
- [ ] Keep the view readable on narrow screens by stacking the two panels vertically.

**Acceptance target:** an operator can identify the main differences between the traces in one screen.

**Verification:** component tests cover the diff summary and responsive stacking behavior.

**Rollback path:** fall back to the existing trace detail page; no backend contract changes.

---

## Task 3: Add an entry point from trace detail

**Files:**
- Modify `apps/frontend/src/pages/TraceDetail.tsx`
- Modify `apps/frontend/src/pages/TraceDetail.test.tsx`

- [ ] Add a `Compare trace` action in the trace detail header.
- [ ] Link the current trace into the compare route as the prefilled left-hand trace.
- [ ] Keep the existing back-navigation behavior unchanged.

**Acceptance target:** a user can jump from a trace to the compare workflow without manually copying the trace ID.

**Verification:** update the trace detail tests to assert the new link and route target.

**Rollback path:** remove the link only; the compare page remains directly addressable by URL.

---

## Task 4: Close the slice

- [ ] Update `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` to mark Trace Comparison complete.
- [ ] Update `docs/agent-context.md` to note that the next detailed plan is no longer Trace Comparison.
- [ ] Move this plan file to `archived/plans/` once the implementation and verification are done.
- [ ] Run the required frontend checks and the repo-local CI gate before pushing.

**Next smallest slice:** Query Workbench.

---

## Completion Note

Implemented `/traces/compare` with route-local left/right trace IDs, a compare entry point from trace detail, side-by-side trace summary panels, and a compact diff summary for path and status changes. Verified with focused Vitest coverage, frontend build, and frontend lint.
