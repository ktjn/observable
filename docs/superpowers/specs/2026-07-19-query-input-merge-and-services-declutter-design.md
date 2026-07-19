# Query input merge, Service pages declutter, testbench deployment markers, Workbench WebLLM fix

Status: Approved
Date: 2026-07-19

## Context

A user bug-report bundle covering the Services view, the Query Workbench, and the
testbench. Investigation found five largely independent pieces of work, ranging
from a one-line bug fix to a real UX design change:

1. Services view is cluttered — redundant signal links, redundant search inputs,
   too many stacked panels.
2. The quick-filter input (`QueryFilterInput`) and the "Ask" NLQ panel
   (`NlqPanel`) are two separate components with two different submission
   modes, doing conceptually overlapping jobs.
3. Testbench never emits deployment markers, even though the marker
   API/rendering already exists elsewhere in the product.
4. The "Ask / Natural Language Query" panel header text is unnecessary
   clutter (resolved by removing the panel itself, see below).
5. Workbench doesn't work with the WebLLM provider — a real bug with a known
   root cause.

## 1. Merged query input (`QueryInput`)

### Problem

`QueryFilterInput` (used on `ServicesPage`, `LogSearch`, `TraceSearch`,
`MetricsSearch`, `InfrastructureInventoryPage`) and `NlqPanel` (used standalone
as the "Ask" panel on `ServiceDetailPage`) are separate components. Both
ultimately call `submitNlqWithProvider`, but every text query — including a
single word like `error` or an obvious field filter like `service:checkout`
— is sent through the LLM interpret/execute path. This costs an LLM round
trip for cases that don't need one.

Query-api already has a deterministic shorthand grammar (ADR-029,
`services/query-api/src/llm_adapter.rs`) that handles exactly these cases:

- `m:<name>` — metric field
- `f:<field>:<val>` / `<field>:<val>` — equality filter
- `op:<type>` — operation override
- quoted or bare word — free-text term, matched via
  `positionCaseInsensitive(body, '<term>') > 0` (substring/"contains"
  matching — there is no real glob engine, so `*error*`, `error*`, and
  `error` all resolve to the same "contains error" query)

Today this grammar only activates when the user manually types `/` first, or
when no LLM is configured. The frontend never invokes it automatically.

### Design

Introduce one component, `QueryInput` (`apps/frontend/src/features/nlq/QueryInput.tsx`),
replacing both `QueryFilterInput` and `NlqPanel` at every call site.

**Client-side mode detection**, run on submit against the trimmed input text:

1. If text already starts with `/` → strip it, treat as **Filter**, unchanged
   passthrough (existing explicit-bypass behavior).
2. Else if text matches the shorthand grammar shape — a single token
   matching `^[A-Za-z_][\w.-]*:\S+$` (`field:value`), `^m:\S+$`, or `^op:\S+$`,
   with no surrounding whitespace-separated additional tokens beyond that
   pattern — treat as **Filter**: prefix with `/` before sending.
3. Else if text is a single token matching `^\*?[\w.-]+\*?$` (one word,
   optionally wrapped in leading/trailing `*`) → treat as **Search**: strip
   any `*`, prefix the bare word with `/` before sending (goes through the
   same shorthand "free-text term" path as case 2).
4. Else → treat as **AI**: send unchanged through the existing NLQ
   interpret/execute call via `submitNlqWithProvider` (unaffected by this
   change; already provider-aware).

Detection is a pure function (`detectQueryMode(text): "filter" | "search" | "ai"`)
with unit tests covering: plain word, `*word*`, `word*`, `*word`, `field:value`,
`m:foo`, `op:rate`, multi-word phrase, a full question, explicit `/`-prefixed
input, and edge cases (empty string, quoted phrase with spaces).

**Mode badge**: once the input has text, a small inline badge next to the
field shows `Filter` / `Search` / `AI` reflecting live detection (recomputed
on each keystroke, not just on submit) so the user can see which path a query
will take before hitting enter.

**Execution semantics carry over from the two source components** based on
how each call site is wired today:

- Call sites that pass `onSubmit`/`onIr` (narrow-the-current-view behavior,
  today's `QueryFilterInput` usage) keep that behavior in `QueryInput` —
  filter/search/AI-interpret results all flow through the same `onSubmit`
  callback with the resolved IR.
- The one `NlqPanel` call site (`ServiceDetailPage`'s "Ask" panel,
  ad-hoc execute + inline visualization) is **removed**, not migrated — see
  Section 2. `QueryInput` does not need an "execute and render a
  visualization inline" mode; that capability now lives only in Workbench.

**Call sites to update:**
- `ServicesPage.tsx` — replace `QueryFilterInput` with `QueryInput`; delete
  the separate plain `<input type="search">` box (redundant once `QueryInput`
  handles bare-word search itself). The health `PillFilter` stays — it's a
  distinct, non-redundant control.
- `LogSearch.tsx`, `TraceSearch.tsx`, `ServiceMetricsWorkspace.tsx` (or
  wherever their `QueryFilterInput` usage lives), `InfrastructureInventoryPage.tsx`
  — swap `QueryFilterInput` → `QueryInput`, same props/callback contract.
- `ServiceDetailPage.tsx` — the `NlqPanel` "Ask" panel is deleted outright
  (Section 2), not replaced with `QueryInput`.

`QueryFilterInput.tsx` and `NlqPanel.tsx` are deleted once all call sites
migrate. `ShorthandHint.tsx` (the hover reference card) is reused as-is,
attached to `QueryInput`.

## 2. Service Detail page — consolidate into tabs

### Current layout (top to bottom)

Header → 4 metric cards → response-time graph → [`Current State` panel +
`Signal Entry Points` panel, side by side] → `ServiceInfraPanel` →
`Ask`/NLQ panel → `ServiceSignalTabs` (Reliability/Logs/Metrics/Traces/Deployments/Alerts).

Two redundancies drove the "too cluttered" complaint:
- `Signal Entry Points`' four links (Traces/Logs/Metrics/Infrastructure)
  duplicate `ServiceSignalTabs` below (Traces/Logs/Metrics) and
  `ServiceInfraPanel` right below it (Infrastructure) — all four are dead
  weight.
- The `Ask`/NLQ panel duplicates Workbench's job with no meaningful UX
  advantage of being embedded here.

### New layout

Header + 4 metric cards + response-time graph stay unchanged at the top.
Everything else becomes a tab in `ServiceSignalTabs`:

- Add an **Infrastructure** tab that renders `ServiceInfraPanel`'s content
  (component reused as-is, just moved from always-rendered to tab content).
- Fold `Current State`'s unique fields (SLO/health-state label, latest
  deployment) into the **Reliability** tab (`ServiceReliabilityTab`) as a
  small summary block at its top — the metric-card row above already
  surfaces health/error-rate/alert-count at a glance, so this isn't lost,
  just relocated to where health detail already lives.
- Delete the `Signal Entry Points` panel entirely.
- Delete the `Ask`/NLQ panel entirely. Add a small "Ask in Workbench →" link
  in the page header (next to "Back to services") that navigates to
  Workbench, optionally pre-scoped to this service if Workbench's routing
  supports an initial-service param (check `QueryWorkbenchPage.tsx`; if not
  trivial, a plain unscoped link is fine — not a blocker for this change).

Resulting tab order: Reliability, Logs, Metrics, Traces, Infrastructure,
Deployments, Alerts.

## 3. Services list page

Delete the standalone `<input type="search">` box and its `search` state
wiring insofar as it's redundant with `QueryInput`'s bare-word search mode
(Section 1). `QueryInput`'s `onIr`/`onSubmit` callback continues to drive
`setSearch`/`setEnvironment`/`setHealthFilter` via `deriveViewFiltersFromIr`,
unchanged. `PillFilter` (health) stays.

## 4. Testbench deployment markers

No backend/API changes — `POST /v1/deployments` and
`PATCH /v1/deployments/{deployment_id}` already exist
(`services/ingest-gateway/src/deployments.rs`) and are already rendered as
chart annotations (`ServiceDeploymentsTab`, `TimeSeriesGraph`'s
`deploymentMarkers` prop).

Add a hook so testbench calls these when it (re)deploys the shop services:
- On deploy start: `POST /v1/deployments` with `service_name`, `environment`,
  `service_version` (derive from the image tag/chart values already used by
  `scripts/testbench.sh` / the Helm chart), `status: "in_progress"`.
- On deploy completion (Helm install/upgrade succeeds, pods healthy):
  `PATCH /v1/deployments/{deployment_id}` with `status: "success"` (or
  `"failed"` if the deploy failed — best-effort, don't block testbench
  teardown on this call failing).

Implementation location: a small hook in `scripts/testbench.sh` around the
existing `helm upgrade --install` invocation, using `curl` against the
ingest-gateway service with the testbench tenant's API key (testbench
already has one for OTLP ingest — reuse it, don't mint a new one unless the
existing key's role can't write to `/v1/deployments`; verify during
implementation).

## 5. Workbench + WebLLM fix

`apps/frontend/src/features/workbench/QueryWorkbench.tsx`'s `runBlock()`
currently calls `submitNlqQuery` (`api/nlq.ts`) directly:

```ts
const response = await submitNlqQuery(tenantId, { base_ir: mergedBaseIr, mode: "execute" });
```

This bypasses `submitNlqWithProvider`, the shared function every other NLQ
surface uses to branch on `config.llm_provider` ("remote" vs "webllm") and
run the two-phase WebLLM prepare/complete flow when applicable. Workbench is
therefore always remote-only regardless of the user's Setup page provider
selection.

Fix: change `runBlock()` to call `submitNlqWithProvider(tenantId, { provider, webllmModel }, { base_ir: mergedBaseIr, mode: "execute" })`,
reading `provider`/`webllmModel` from `getConfig(tenantId)` the same way
`QueryFilterInput`/`QueryInput` already do (see commit `fe45cae6`, which did
this exact fix for `QueryFilterInput` — mirror that pattern). No other
behavior change; this is a drop-in call-site fix.

## Out of scope

- No changes to the underlying shorthand grammar or substring-match SQL
  generation in query-api — Section 1 only adds a client-side heuristic that
  routes into grammar/paths that already exist.
- No new deployment-marker API or schema — Section 4 is testbench wiring
  only.
- No changes to WebLLM's engine/model itself — Section 5 is a call-site fix.
- Workbench pre-scoping to a specific service (the "Ask in Workbench" link
  in Section 2) is a nice-to-have, not required for this design to be
  considered complete if Workbench's routing doesn't already support it.

## Testing

- Unit tests for `detectQueryMode` covering all cases listed in Section 1.
- Existing `QueryFilterInput`/`NlqPanel` test files
  (`SignalExplorer.test.tsx`, `queryFilters.test.ts`, any `NlqPanel.test.tsx`)
  migrated to exercise `QueryInput` instead, plus new tests for mode-badge
  rendering and the filter/search bypass paths.
- `ServiceDetailPage` — update/extend existing tests for the new tab set
  (Infrastructure tab present, Signal Entry Points and Ask panel gone).
- `ServicesPage` — update tests to confirm the standalone search box is gone
  and bare-word search still filters the table via `QueryInput`.
- Workbench — a test asserting `runBlock` calls `submitNlqWithProvider` with
  the configured provider (regression test for the bug itself).
- Testbench — manual verification: run `scripts/testbench.sh`, confirm a
  deployment marker appears on a service's chart after a shop-service
  redeploy.
