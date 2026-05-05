# P3-S10 Design: Infrastructure Correlation from Service and Trace Views

**Date:** 2026-04-24
**Slice:** P3-S10
**Source spec:** `spec/05-frontend.md` §9.4 Infrastructure Correlation

---

## Goal

Users can navigate from a service, trace, or log view to correlated host/pod/container infrastructure detail pages using OTel resource attributes already present in API responses.

---

## Approach

Frontend-only. The backend `Span` domain struct already includes `resource_attributes` (a full JSON map) in every trace and span response. `LogRecord` responses also carry `resource_attributes`. No backend changes are needed — the gap is entirely in the frontend TypeScript interfaces and UI surfaces.

A shared `infraLinks()` utility maps OTel resource attribute keys to `/infrastructure/:type/:id` navigation URLs, mirroring the same key mapping used by the backend `discovery.rs` `attribute_sql_expr` logic.

---

## Data Layer

### Interface additions

`apps/frontend/src/api/traces.ts` — add to `Span`:
```ts
resource_attributes?: Record<string, unknown>;
```

`apps/frontend/src/api/logs.ts` — add to `LogRecord`:
```ts
resource_attributes?: Record<string, unknown>;
```

### Link builder utility

New file: `apps/frontend/src/utils/infraLinks.ts`

```ts
export interface InfraLink {
  label: string;
  href: string;
}

export function infraLinks(attrs: Record<string, unknown>): InfraLink[]
```

Mapping (in priority order — only keys that are present and non-empty produce a link):

| OTel attribute key | Fallback key | Entity type | Link path |
|---|---|---|---|
| `k8s.pod.name` | — | `pod` | `/infrastructure/pod/<value>` |
| `host.name` | `host.id` | `host` | `/infrastructure/host/<value>` |
| `k8s.namespace.name` | — | `namespace` | `/infrastructure/namespace/<value>` |
| `k8s.cluster.name` | — | `cluster` | `/infrastructure/cluster/<value>` |
| `container.name` | `container.id` | `container` | `/infrastructure/container/<value>` |

Entity ID values are `encodeURIComponent`-encoded. Returns `[]` when `attrs` has no recognisable infra keys.

The function is pure with no I/O and cannot throw.

---

## Three Surfaces

### 1. Service Detail — Inline Infrastructure Panel (Overview tab)

**Component:** `ServiceInfraPanel` (new), rendered below the RED metrics panel in `ServiceDetailPage.tsx`.

**Data:** Calls existing `listInfrastructure({ service: serviceId })`. No new API endpoint.

**Renders:**
- A compact list of up to 10 entity cards, each showing:
  - Entity type badge (host / pod / container / etc.)
  - Display name as a link to `/infrastructure/:type/:id`
  - Health state indicator dot
  - CPU and memory usage if available
- Empty state: "No infrastructure entities observed for this service."
- Error state: "Could not load infrastructure." (muted, non-crashing)

**Out of scope:** Pagination, sorting controls, full inventory duplication.

---

### 2. Trace Detail — Trace-Level Infrastructure Summary

**Component:** `TraceDetail.tsx` (modified).

**Logic:**
1. Collect `resource_attributes` from every span in the trace.
2. Merge all attributes maps; deduplicate by entity identity (e.g. same `k8s.pod.name` value).
3. Call `infraLinks()` on the merged set.
4. Render a horizontal row of pill links above the span waterfall (e.g. `pod: checkout-pod-1`, `host: node-3`).

**Empty case:** If no infra attributes are present across any span, the section is omitted entirely — no empty state message.

---

### 3. Log Explorer — Inline Resource Attribute Badges

**Component:** Log list row (modified).

**Logic:** For each `LogRecord`, call `infraLinks(record.resource_attributes ?? {})`. If the result is non-empty, render a row of small linked badges appended after the service name column. Each badge links to the corresponding infrastructure detail page.

**Empty case:** Rows with no recognisable infra attributes show nothing extra.

---

## Error Handling

| Surface | Failure mode | Behaviour |
|---|---|---|
| `ServiceInfraPanel` | `listInfrastructure` rejects | Muted error message; overview tab remains functional |
| Trace infra summary | Missing/null `resource_attributes` | Section omitted |
| Log badges | Missing/null `resource_attributes` | No badges rendered |
| `infraLinks()` | Unrecognised or null attribute values | Silently skipped |

---

## Testing

| Test file | Coverage |
|---|---|
| `infraLinks.test.ts` | pod+host present → two links; unrecognised attrs → `[]`; empty attrs → `[]`; fallback keys (`host.id`, `container.id`); values are URL-encoded |
| `ServiceInfraPanel.test.tsx` | Linked entity list from mock API; empty state |
| `TraceDetail.test.tsx` | Infra pills render when spans have `resource_attributes`; section absent when no infra attrs |
| Log list test | Badges render for records with `k8s.pod.name`; no badges without infra attrs |

No new backend tests — backend is unchanged.

---

## Files Expected to Change

- `apps/frontend/src/api/traces.ts` — add `resource_attributes` to `Span`
- `apps/frontend/src/api/logs.ts` — add `resource_attributes` to `LogRecord`
- `apps/frontend/src/utils/infraLinks.ts` — new utility
- `apps/frontend/src/utils/infraLinks.test.ts` — new tests
- `apps/frontend/src/pages/ServiceDetailPage.tsx` — add `ServiceInfraPanel`
- `apps/frontend/src/components/ServiceInfraPanel.tsx` — new component
- `apps/frontend/src/components/ServiceInfraPanel.test.tsx` — new tests
- `apps/frontend/src/pages/TraceDetail.tsx` — add trace-level infra summary section
- `apps/frontend/src/pages/TraceDetail.test.tsx` — extend with infra attr cases
- Log list component — add infra badges per row, extend tests

## Out of Scope

- Backend changes to span/log/trace response shapes
- New backend API endpoints
- Infrastructure tab in service detail (overview panel only)
- Per-span infrastructure links in trace detail (trace-level only)
- Pagination or sorting in the service infra panel

---

## ADR/Spec Sync

No ADR update needed. This slice adds frontend navigation using an existing backend API and existing response fields. It does not change architecture, technology choice, deployment model, data model, security model, or roadmap scope.
