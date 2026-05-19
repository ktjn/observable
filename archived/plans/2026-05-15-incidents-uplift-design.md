# Incidents Page Uplift — Design Spec

**Date:** 2026-05-15
**Scope:** `IncidentsPage.tsx` + `IncidentDetailPage.tsx`
**Reference:** `AlertsPage.tsx` (established uplift pattern)

---

## Goal

Bring both Incidents pages to visual and structural parity with the Alerts & SLOs page — consistent layout primitives, CSS variables, filter pills, row tinting, and MetricCard summary row. No new features; pure uplift.

---

## IncidentsPage

### Layout

Replace `<div className="space-y-4">` with `<section className="page-stack">`.

Add a `page-header` block:
```
eyebrow: "Reliability"
h1:      "Incidents"
```

### MetricCard summary row

Five cards in a `grid grid-cols-1 gap-4 sm:grid-cols-5`:

| Label | Value | Tone |
|-------|-------|------|
| Total | `items.length` | `info` |
| Triggered | count where `status === "triggered"` | `bad` if > 0, else `good` |
| Acknowledged | count where `status === "acknowledged"` | `warn` if > 0, else `info` |
| Resolved | count where `status === "resolved"` | `good` |
| MTTR | mean minutes from `triggered_at` → `resolved_at` across resolved incidents, computed client-side; displayed as `"Xm"` or `"—"` if none | `info` |

MTTR is computed as:
```ts
const resolved = items.filter(i => i.resolved_at);
const mttrMin = resolved.length
  ? Math.round(resolved.reduce((sum, i) => {
      return sum + (new Date(i.resolved_at!).getTime() - new Date(i.triggered_at).getTime());
    }, 0) / resolved.length / 60_000)
  : null;
```

### Filter pills

Replace `<Tabs>` with inline filter pills matching the AlertsPage `ruleFilter` pattern:

Pills: **All** / **Triggered** / **Acknowledged** / **Resolved** — each showing its count in parentheses.

Active pill colour:
- All → `var(--brand)`
- Triggered → `var(--bad)`
- Acknowledged → `var(--warn)`
- Resolved → `var(--good)`

### Table

Keep existing columns (Title / Severity / Status / Triggered / Resolved).

Row class: `modern-table-row border-l-2` with left-border tinting:
- `triggered` → `border-l-[var(--bad)]`
- `acknowledged` → `border-l-[var(--warn)]`
- `resolved` → `border-l-transparent`

All `text-muted-foreground` → `text-[var(--muted)]`.

Timestamps use `useTimeDisplay`'s `format` function (not `toLocaleString()`).

Panel: `title="Incidents"` `eyebrow="Active and historical"`.

---

## IncidentDetailPage

### Layout

Replace `<div className="space-y-4">` with `<section className="page-stack">`.

Replace `<Toolbar>{data.title}</Toolbar>` with a `page-header` block:
```
eyebrow: "Incident"
h1:      {data.title}
actions: severity badge + status badge
```

### Metadata panel

Field labels use the `field-label` CSS class. All `text-muted-foreground` → `text-[var(--muted)]`.

Timestamps use `format` from `useTimeDisplay`.

### Timeline

Replace emoji icons with monospace glyphs:

| event_type | glyph |
|---|---|
| `triggered` | `▸` |
| `alert_fired` | `!` |
| `alert_resolved` | `✓` |
| `acknowledged` | `◎` |
| `comment` | `·` |
| `status_change` | `→` |
| `deployment_linked` | `↑` |
| *(default)* | `·` |

Glyph rendered in a fixed-width `font-mono` span. All `text-muted-foreground` → `text-[var(--muted)]`. Timestamps use `format`.

---

## Files changed

| File | Changes |
|------|---------|
| `apps/frontend/src/features/incidents/IncidentsPage.tsx` | Layout, MetricCards, filter pills, table row tinting, CSS vars, timestamps |
| `apps/frontend/src/features/incidents/IncidentDetailPage.tsx` | Layout, page-header, CSS vars, timeline glyphs, timestamps |

No new files. No API changes. No test changes required beyond snapshot updates if any.
