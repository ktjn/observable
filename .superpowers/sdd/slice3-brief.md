# Task Brief — Slice 3: De-jargon Filter Placeholders & Timezone Label

## Context
This is Slice 3 (P1) of the UI Usability Remediation — a fast win. Two jargon-exposure issues:
1. Eight filter inputs across the app expose "raw NLQ IR JSON" (an internal intermediate representation) to end users
2. The default timezone label reads "ISO8601 Client TZ [ms]" — engineer jargon in prime header space

## Task
Remove "raw NLQ IR JSON" from user-facing placeholders and soften the timezone labels.

---

## Part A: De-jargon filter placeholders

### Files to modify
All occurrences of "raw NLQ IR" in these files:
- `apps/frontend/src/components/shared/SignalExplorer.tsx` (line 95, 100)
- `apps/frontend/src/components/LogLiveTail.tsx`
- `apps/frontend/src/features/metrics/ServiceMetricsWorkspace.tsx`
- `apps/frontend/src/features/nlq/QueryFilterInput.tsx`
- `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`
- `apps/frontend/src/pages/ProductAreaPage.tsx`
- `apps/frontend/src/pages/ServicesPage.tsx`
- `apps/frontend/src/pages/ServiceTopologyPage.tsx`

### Replacement rules
- Replace every occurrence of `"raw NLQ IR JSON"` (or similar "NLQ IR" jargon) in placeholder strings with nothing — just remove the "or raw NLQ IR JSON" clause entirely
- Examples:
  - `"Filter traces with NLQ or raw NLQ IR JSON"` → `"Filter traces with natural language"`
  - `"Filter this view with natural language or raw NLQ IR JSON"` → `"Filter this view with natural language"`
  - `"Filter ${title.toLowerCase()} with NLQ or raw NLQ IR JSON"` → `` `Filter ${title.toLowerCase()} with natural language` ``
- Keep the natural-language example; only drop the IR mention
- Power-user raw-IR entry remains supported but unadvertised (no UI change beyond the placeholder text)
- After changes: `grep -r "NLQ IR" apps/frontend/src/` must return zero user-facing string matches (only code-internal identifiers are acceptable, not user-visible strings)

---

## Part B: Soften timezone labels

### File to modify
`apps/frontend/src/lib/timeDisplay.tsx`

### Current labels (lines 12-18)
```
{ value: "iso-local-ms", label: "ISO8601 Client TZ [ms]" },
{ value: "iso-utc-ms",   label: "ISO8601 UTC [ms]" },
{ value: "iso-local-ns", label: "ISO8601 Client TZ [ns]" },
{ value: "iso-utc-ns",   label: "ISO8601 UTC [ns]" },
{ value: "unix-ms",      label: "Unix time [ms]" },
{ value: "unix-ns",      label: "Unix time [ns]" },
```

### New labels (replace exactly)
```
{ value: "iso-local-ms", label: "Local time (ms)" },
{ value: "iso-utc-ms",   label: "UTC (ms)" },
{ value: "iso-local-ns", label: "Local time (ns)" },
{ value: "iso-utc-ns",   label: "UTC (ns)" },
{ value: "unix-ms",      label: "Unix timestamp (ms)" },
{ value: "unix-ns",      label: "Unix timestamp (ns)" },
```

- The `TimeFormat` type values (`"iso-local-ms"` etc.) do NOT change — only the `label` display strings
- The `DEFAULT_TIME_FORMAT` (`"iso-local-ms"`) does NOT change

---

## Tests

### Verify zero NLQ IR in user-facing strings
Add or update tests in the most appropriate existing test file, OR verify via a grep check in the implementer report. At minimum, run:
```bash
grep -r "NLQ IR" apps/frontend/src/ --include="*.tsx" --include="*.ts"
```
and confirm zero user-facing occurrences.

### Timezone label test
Check for an existing test file for `timeDisplay` in `apps/frontend/src/`. If one exists, update it. If not, add a brief test that the `TIME_FORMAT_OPTIONS` default label (`iso-local-ms`) now reads `"Local time (ms)"` — in `apps/frontend/src/lib/timeDisplay.test.ts`.

---

## Verification
1. `npm run typecheck` from `apps/frontend/` — must pass
2. `npm test` from `apps/frontend/` — all existing tests must pass; new timezone label test must pass
3. Run `grep -r "NLQ IR" apps/frontend/src/` — must return zero results

## Commit
```
fix(ui): remove NLQ IR jargon from filter placeholders and soften timezone labels
```

## Report Contract
Write full report to: `.superpowers/sdd/slice3-report.md`
Return only: status (DONE/DONE_WITH_CONCERNS/NEEDS_CONTEXT/BLOCKED), commit hash, one-line test summary, concerns.
