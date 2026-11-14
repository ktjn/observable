# Phase 3 detailed plan: Experience shell

**Status:** S1–S2 delivered (`10 / 22` verified); S3–S6 remaining.
**Inputs:** [ROADMAP.md](../../ROADMAP.md), [business-spec-adoption.md](business-spec-adoption.md), [phase2-exit-review.md](phase2-exit-review.md), [ux-workflows.yaml](../../spec/business/experience/ux-workflows.yaml), [acceptance-tests.yaml](../../spec/business/verification/acceptance-tests.yaml), `spec/business/areas/{13,20,21,22,24,25}-*.yaml`.
**Branch policy:** One `FEATURE.md` slice per branch/PR into `2.0`; every change passes `bash scripts/spec-gate.sh` (and `bash scripts/local-ci.sh` for code changes).

## 1. Scope and exit

Phase 3 verifies **22 P0 requirements** (all `holder: platform`), **4 critical UX workflows** (`UX-01`, `UX-02`, `UX-09`, `UX-10`), and **WCAG 2.2 AA** accessibility across the experience shell:

| Slice | Capability / Area | P0 IDs (`22` total) | Current gap | Primary exit proof |
|---|---|---|---|---|
| **S1** | `navigate-operational-workspaces-ui` (`observable-web`) | `OBS-20-01`, `OBS-20-02`, `OBS-20-03`, `OBS-20-04`, `OBS-20-08`, `OBS-21-01`, `OBS-21-03`, `OBS-21-08` | `8` partial | Persistent scope bar, critical-service & incident-ownership focus, lossless drilldown & `UX-01` back navigation |
| **S2** | `report-service-status` (`observable-query`, `observable-web`) | `OBS-13-01`, `OBS-13-02` | `2` partial | Accountable-service summary & range-aligned charts/histograms with scope/period disclosure |
| **S3** | `explore-telemetry-ui` (`observable-web`) | `OBS-22-01`, `OBS-22-02`, `OBS-22-03`, `OBS-22-05`, `OBS-22-11` | `5` partial | Baseline comparison, unit/aggregation labels, facet & bucket drilldown, `UX-02` refresh-safe inspection |
| **S4** | `interaction-states-ui` (`observable-web`) | `OBS-24-03`, `OBS-24-04`, `OBS-24-05`, `OBS-24-09` | `2` partial, `2` missing | Unified empty/error/mutation states, keyboard & screen-reader announcements (`WCAG-2.2-AA`, `UX-09`) |
| **S5** | `access-aware-ui` (`observable-web`) | `OBS-25-01`, `OBS-25-02`, `OBS-25-03` | `2` partial, `1` missing | Capability-gated actions, existence hiding on search/links, elevated-access indicator & expiry (`UX-10`) |
| **S6** | Exit review & spec gate | All `22` above | `0 / 22` verified | Strict per-criterion `Evidence` in `spec/system.yaml`, `phase3-exit-review.md`, `spec-gate.sh` green (`22 / 22`) |

## 2. Detailed slice design

### S1 — Global Scope, Navigation & Workspace Entry (`8` P0)

- **Workspace & reporting entry (`OBS-20-01`, `OBS-21-01`, `OBS-21-03`):**
  - Update [AppShell.tsx](../../apps/frontend/src/components/AppShell.tsx) navigation so operational workspaces include service overview/reporting, critical-service focus, and customer-experience/service-status entry without duplicating routes.
  - Surface accountable ownership, business impact, and next action in [HomePage](../../apps/frontend/src/pages/HomePage.tsx) and incident summaries (`OBS-21-03`).
- **Unified search (`OBS-20-02`):**
  - Extend shell search to resolve business name, operational name, domain, owner, and trace ID, respecting caller authorization (`UX-01` `ux-01-search-service`: unauthorized matches produce an empty result, never leaking existence).
- **Persistent global scope (`OBS-20-03`, `OBS-20-04`, `OBS-20-08`, `OBS-21-08`):**
  - Ensure active tenant, `environment`, `time-range`, and active `filters` remain visible and editable in every workspace view (`OBS-20-04`).
  - Preserve scope across overview → service detail → incident → traces/logs drilldowns (`OBS-20-08`, `OBS-21-08`) and restore prior overview/detail scope + selection on `back` (`ux-01-incident-back`).
- **Verification:** Vitest unit tests for scope-store persistence + Playwright `UX-01` suite (`apps/frontend/e2e/navigation.spec.ts`, `service-overview.spec.ts`) covering positive transitions and negative `lost-context` / `restricted-service` / `restricted-match` paths.

### S2 — Service Reporting & Scoped Summary (`2` P0)

- **Accountable service summary (`OBS-13-01`):**
  - Filter or group Home/Overview status by accountable/owned services and business criticality using `report-service-status` data, keeping scope and freshness visible.
- **Range-aligned charts & histograms (`OBS-13-02`):**
  - Bind overview and service histograms to the shared global `time-range` and `environment`, disclosing evaluated period and partial coverage.
- **Verification:** `query-api` integration tests (as needed for scoped accountable filtering) + frontend Vitest/Playwright assertions on `OBS-13-01` and `OBS-13-02` criteria.

### S3 — Visual Exploration & Baseline Comparison (`5` P0)

- **Period & baseline comparison (`OBS-22-01`):**
  - Extend [GlobalDateRangePicker.tsx](../../apps/frontend/src/components/GlobalDateRangePicker.tsx) and chart wrappers to support aligned baseline comparison (`ux-02-compare-baseline`), explicitly calling out period-length differences, units, and missing-data gaps.
- **Facets, units, aggregation & bucket drilldown (`OBS-22-02`, `OBS-22-03`, `OBS-22-05`, `OBS-22-11`):**
  - Standardize chart metadata so every metric/histogram view displays explicit units and aggregation (`p95`, `count`, `rate`, etc.) (`OBS-22-05`).
  - Preserve time context during incremental facet filtering (`OBS-22-02`, `ux-02-refine-filter`) and bucket/interval selection (`OBS-22-03`, `OBS-22-11`, `ux-02-transition`).
  - Keep active interval selection pinned during manual/auto refresh (`ux-02-refresh`).
- **Verification:** Playwright `UX-02` suite covering interval selection, facet refinement, baseline comparison, refresh stability, and `unauthorized` / `stale` failure modes.

### S4 — Baseline Interaction States, Action Feedback & Accessibility (`4` P0)

- **Reusable state & feedback primitives (`OBS-24-03`, `OBS-24-04`, `OBS-24-09`):**
  - Audit and consolidate shared state components under `apps/frontend/src/components/ui/` (`state.tsx` and action banners/toasts):
    - `OBS-24-03`: Distinguish "no matching records in scope" from unconfigured/missing data or restricted results, with a clear path to clear filters or configure.
    - `OBS-24-04`: Replace ad-hoc error strings with structured, recoverable error states (actionable next step + retry without losing form/filter state).
    - `OBS-24-09`: Provide explicit, accessible confirmation and failure feedback for mutations (maintenance windows, rules, incident acknowledgement, dashboard edits).
- **Accessibility & assistive technology (`OBS-24-05`, `UX-09`):**
  - Verify keyboard-only operation, visible focus indicators, landmarks, ARIA live announcements for async state transitions/acknowledgements, and zero `axe-core` WCAG 2.2 AA violations across all shell views and `UX-09` (`ux-09-success`, `ux-09-failure`).
- **Verification:** Vitest component tests in `apps/frontend/src/components/ui/` + Playwright `accessibility.spec.ts` / `UX-09` browser checks.

### S5 — Access-Aware UI, Existence Hiding & Elevated Access (`3` P0)

- **Capability-gated actions (`OBS-25-01`):**
  - Expose UI actions only when the caller's session capabilities permit them; disable/withhold restricted actions cleanly without offering broken controls.
- **Existence hiding (`OBS-25-02`):**
  - Ensure unauthorized deep links, shared investigation URLs (`ux-10-entry`), and restricted search queries (`ux-10-search-restricted`) render the baseline `unauthorized` / `empty` state without leaking record existence, IDs, or counts.
- **Active tenant & elevated access (`OBS-25-03`, `UX-10`):**
  - Extend [UserMenu.tsx](../../apps/frontend/src/components/UserMenu.tsx) / shell header to display elevated-access status prominently when active, handle elevation expiry (`elevation-expiry`) cleanly by revoking privileged views/actions, and never perform elevation silently.
- **Verification:** Playwright `UX-10` suite (`shared-investigation-link`, `search-restricted`, `elevation-expiry`) + Vitest unit tests for access-aware hooks/components.

### S6 — Phase 3 Exit Review & Verification Gate

1. Re-run `apps/frontend` unit (`vitest`), `typecheck`, `lint`, `build`, and browser/a11y (`playwright`) suites.
2. Attach strict per-criterion `Evidence` records in [spec/system.yaml](../../spec/system.yaml) for all `22` Phase 3 P0 requirements (covering both `Acceptance:` and `Negative:` criteria) and set `status: verified` in `spec/business/areas/{13,20,21,22,24,25}-*.yaml`.
3. Update [business-spec-gap.yaml](business-spec-gap.yaml) and publish `docs/plans/phase3-exit-review.md`.
4. Confirm `bash scripts/spec-gate.sh` reports `phase 3: total 22, verified 22, partial 0, missing 0` and `bash scripts/local-ci.sh` passes before opening the closeout PR.

## 3. Component & contract rules

- Reuse existing shared primitives in `apps/frontend/src/components/ui/` and `apps/frontend/src/components/` before introducing new components (`AGENTS.md`).
- Never treat UI controls as authorization enforcement: server-side `decide-access` / tenant checks remain authoritative ([ADR-008](../../spec/adr/ADR-008-authorization-model.md)).
- Any change to `spec/system.yaml` capability contracts follows `system-spec/FEATURE.md` (`--impact`, version bump if needed, `bash scripts/spec-gate.sh`).
