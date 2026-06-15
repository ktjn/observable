# ADR-032: Adopt Modelable as the Type-Mapping Source of Truth

**Date:** 2026-06-15
**Status:** Accepted
**Authors:** ktjn
**Deciders:** Project Stakeholders
**Review date:** 2027-06-15

## Context

An audit at the start of this migration found 19 domain types (`libs/domain/src/*.rs`), 47
backend API request/response types (`services/query-api/src/*.rs`), and 41 hand-written
frontend TypeScript interfaces (`apps/frontend/src/api/*.ts`), with only ~6 explicit
`From`/`Into` mappings tying these layers together. ClickHouse/Postgres row structs, domain
structs, API request/response structs, and frontend TS interfaces were each maintained by
hand, with no mechanism to detect drift between them.

[modelable](https://github.com/ktjn/modelable) is a declarative compiler: canonical domain
models are defined once in `.mdl` files under `models/`, and per-language artifacts (Rust,
TypeScript, JSON Schema, SQL) are generated with field-level lineage tracking and
breaking-change detection (`modelable lineage <Type@version>`).

## Decision

Adopt modelable `.mdl` files as the canonical source of truth for shared domain/wire types,
migrated domain-by-domain (Phase 2 pilot + Phase 3 full migration, per
`docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`). Specifically:

- Generated Rust/TypeScript artifacts are **committed to the repo** (not built in CI),
  verified clean via a `modelable compile` diff-check.
- **Per-domain rule:** only replace types representing canonical domain/wire contracts.
  Handler-level aggregation/wrapper types with no 1:1 generated equivalent (e.g.
  `TraceResponse`, `LogListResponse`, `IncidentListResponse`) stay hand-written.
- `@wire(...)` hints (e.g. `json.fieldCase: "snake_case"`) bridge representation gaps for a
  single target without affecting others.
- A Python/uv toolchain plus a `modelable` checkout is a **dev-time-only** dependency for
  editing `.mdl` sources or regenerating artifacts — not a CI/build dependency for unrelated
  changes.

## Current State

| Domain | `.mdl` | Generated TS | Generated Rust | Notes |
|---|---|---|---|---|
| tracing (Span/SpanEvent) | `models/tracing.mdl` | yes (`generated/tracing/`) | yes — `db`-projection Row types only (`generated/tracing.rs`) | `Span`/`SpanEvent`/enums + From/Into hand-written (Phase 2.3 scope note) |
| logs (LogRecord) | `models/logs.mdl` | yes | yes — `LogRow` alias (`generated/logs.rs`) | `LogRecord` hand-written, From/Into to Row hand-written |
| metrics (MetricPoint) | `models/metrics.mdl` | yes | no | `MetricSeries`/`MetricPointRow`/etc. hand-written — blocked on backlog #1-3 |
| notifications (NotificationChannel) | `models/notifications.mdl` | yes | no | handler types + `From` impl hand-written — backlog #3 |
| admin/members (Member) | `models/admin.mdl` | yes | no | `joined_at` timestamp — backlog #5 |
| slos (SloDefinition) | `models/slos.mdl` | yes | no | timestamp fields — backlog #5 |
| schemas | — | — | — | **3.5b deferred** — no frontend consumer for `SchemaEntry`/`SemanticAnnotation` |
| incidents (Incident/IncidentEvent) | `models/incidents.mdl` | yes | no | timestamp fields — backlog #5 |
| alerts (AlertRule/Firing) | `models/alerts.mdl` | yes | no | timestamp fields — backlog #5 |
| dashboards (Dashboard/DashboardPanel/...) | `models/dashboards.mdl` | yes | no | timestamp fields — backlog #5; cross-model imports — backlog #7 |
| nlq/visualization (NlqIr/NlqFilter/NlqTimeRange/FieldRole) | `models/nlq.mdl` | yes | no | `Option<T>` nulls — backlog #8; `array<enum(...))` — backlog #9 |

## Known Limitations

Condensed from the Phase 1 backlog in
`docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` ("Phase 1
backlog" section); see that section for full detail on each item.

1. Array-element `rust.type` hints not supported (blocks `Vec<u64>` histogram fields).
2. No non-optional default-empty-array projection fields.
3. `enum(...)` always emits Rust `String`, never a real enum.
4. Duplicate `binding` declarations break from-scratch registry builds.
5. `timestamp` emits Rust `String`, not `chrono`-compatible.
6. `enum(...)` members can't start with a digit (blocks e.g. `"5m"` preset unions).
7. TS emitter doesn't emit imports for cross-model `NamedType` references (manual workaround
   in place for `dashboards.Dashboard.v1.ts`).
8. `Option<T>` can't express "always-present, possibly-null" (`T | null`) — only omittable
   (`T | undefined`).
9. `array<enum(...))` emits invalid TypeScript operator precedence.

These gaps are why 8/10 domains' Rust layers remain hand-written (with lineage doc comments
only) and why `NlqIr`/`FieldRole` etc. needed hand-written TS extensions.

## Consequences

- **Easier:** TypeScript types for all 10 in-scope domains can no longer silently drift from
  their Rust counterparts — `modelable lineage` proves every generated field traces to its
  `.mdl` source. Adding a field to a migrated domain means editing one `.mdl` file and
  regenerating, not hand-editing N call sites.
- **Harder:** Rust-side migration is incomplete for 8/10 domains pending modelable emitter
  work (Phase 1 backlog). Anyone editing `.mdl` files needs a modelable checkout plus
  Python/uv locally (dev-time only, not CI-gating per "Resolved Decisions" in the migration
  plan).

## Alternatives Considered

- **Status quo (hand-written, manually audited):** Rejected — this is the drift problem that
  motivated the migration; manual audits don't scale and don't prevent regressions.
- **OpenAPI/protobuf-based codegen:** Rejected — neither models the `db` ↔ canonical ↔
  `reply`/`event` auto-projection distinction Observable's layered architecture needs
  (ClickHouse/Postgres row shapes differ from API response shapes differ from domain
  structs); adopting either would require building that projection concept from scratch,
  which is exactly modelable's value-add.

## Related

- `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — full
  migration plan, Phase 1 backlog, per-domain design specs.
- `ADR-030: Timestamp Representation` — the Unix-nanoseconds-as-string convention is
  **unaffected** by this migration; no `.mdl` model currently overrides it (Phase 1 backlog
  item 5 is about a *different* `timestamp` IDL type used for `created_at`/`joined_at`-style
  fields, not the ADR-030 telemetry-timestamp convention).
- Each domain's design spec under `docs/superpowers/specs/2026-06-1{2,3,4,5}-*-modelable-migration-design.md`.
