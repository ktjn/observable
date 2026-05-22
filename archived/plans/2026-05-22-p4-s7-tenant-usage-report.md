# P4-S7 Tenant Usage and Cost Report - Archived Implementation Plan

> Status: Completed on 2026-05-22.
>
> For historical reference only. The active roadmap has already been updated to mark P4-S7 complete.

**Goal:** Give operators a tenant-scoped usage and cost report for one billing interval, using the existing global time range as the interval selector.

**Result:** `GET /v1/tenants/usage-report?from=...&to=...` now aggregates existing ClickHouse telemetry volume and PostgreSQL control-plane activity over the selected interval. The frontend turns the current `/admin` surface into a real billing/usage report view with summary cards and a breakdown panel. This slice stays read-only and does not add a billing/invoicing data model.

## Closure Notes

- Backend route: `services/query-api/src/usage.rs`
- Route registration: `services/query-api/src/main.rs`
- Backend integration coverage: `services/query-api/tests/http_api_integration.rs`
- Frontend API client: `apps/frontend/src/api/usage.ts`
- Frontend page: `apps/frontend/src/features/admin/BillingReportPage.tsx`
- Frontend wiring: `apps/frontend/src/pages/AdminPage.tsx`
- Agent context refresh: `docs/agent-context.md`
- Roadmap update: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`

## Verification

- `cargo fmt --all`
- `cargo test -p query-api --test http_api_integration usage_report`
- `npm test -- --run src/pages/AdminPage.test.tsx`
- `npm run typecheck`
- `npm run lint`
- `npm run build`
- `npm test -- --run`
- `bash scripts/local-ci.sh`

## Checkpoint Answer

Yes: the report has enough signal for a first-pass usage index. Dollar-rate conversion and export remain follow-up slices.

## Next Smallest Slice

Add a more explicit signal-cost breakdown or CSV export once the first usage report is stable.
