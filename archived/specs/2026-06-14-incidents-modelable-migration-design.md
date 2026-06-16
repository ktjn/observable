# Incidents Domain: Modelable Migration Design

**Date:** 2026-06-14
**Status:** Approved
**Scope:** Phase 3 step 3.6 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/incidents.mdl` for the `Incident` and `IncidentEvent` shapes, generate and commit TypeScript artifacts, and wire them into `apps/frontend/src/api/incidents.ts`. Document why `IncidentListResponse`/`IncidentDetailResponse` remain hand-written, and fix the resulting type-fallout in `IncidentsPage.tsx`/`IncidentsPage.test.tsx`.

## Context

`services/query-api/src/incidents.rs` defines four shapes:

- `IncidentItem` (`#[derive(Serialize)]`): `incident_id: Uuid`, `title/severity/status: String`, `triggered_at: DateTime<Utc>`, `resolved_at: Option<DateTime<Utc>>`, `triggered_by_rule_id: Option<Uuid>`. This is the API response shape for the incidents list (`handle_list_incidents`). The Postgres row (`IncidentRow`, `#[derive(sqlx::FromRow)]`) is a separate hand-written struct with the same fields, converted via a manual field-by-field map in `list_incidents`.
- `IncidentEventItem` (`#[derive(Serialize)]`): `event_time: DateTime<Utc>`, `event_type/actor: String`, `message: Option<String>`. Embedded as `IncidentDetailResponse.timeline: Vec<IncidentEventItem>`. Likewise has a separate hand-written row (`IncidentEventRow`, `#[derive(sqlx::FromRow)]`).
- `IncidentListResponse { items: Vec<IncidentItem> }` — list wrapper.
- `IncidentDetailResponse` (`#[derive(Serialize)]`): a join+aggregation response from `get_incident` — superset of `IncidentItem`'s fields (`incident_id`, `title`, `severity`, `status`, `triggered_at`, `resolved_at`, `triggered_by_rule_id`) plus `dedup_key: String`, `runbook_url: Option<String>`, `rule_name: Option<String>`, `impacted_service: Option<String>`, and `timeline: Vec<IncidentEventItem>`. Backed by `IncidentDetailRow` (`#[derive(sqlx::FromRow)]`, a join of `incidents`/`alert_rules`/`slo_definitions`).

`apps/frontend/src/api/incidents.ts` hand-writes matching `IncidentItem`, `IncidentEventItem`, `IncidentListResponse`, `IncidentDetailResponse` interfaces (`resolved_at`/`triggered_by_rule_id`/`runbook_url`/`rule_name`/`impacted_service`/`message` all as `T | null`).

### Why TS-only ("Option B") — reusing Phase 1 backlog item 5

`severity`/`status`/`event_type` are plain `String` in Rust, matching what `enum(...)`/`string` generates (no regression). `triggered_at`, `resolved_at`, `event_time` are `Option<DateTime<Utc>>`/`DateTime<Utc>`, required for `sqlx::FromRow` row-mapping in `IncidentRow`/`IncidentEventRow`/`IncidentDetailRow` (the rows aren't modeled here, but `IncidentItem`/`IncidentEventItem` are built field-by-field from those rows in Rust, so generating them in Rust would still force a `String`-vs-`DateTime<Utc>` mismatch at the conversion site). modelable's `timestamp` primitive emits as Rust `String` (Phase 1 backlog item 5, recorded in 3.4). So, same as 3.2-3.5, this migration is **TS-only**: generate TypeScript artifacts, re-export them into `incidents.ts`, and add lineage doc comments to the Rust `IncidentItem`/`IncidentEventItem` without changing their code. No new backlog item needed.

### Scope: `Incident`/`IncidentEvent` only, not `IncidentDetailResponse`

Per the "per-domain rule" (handler-local request/aggregation shapes may stay hand-written): `IncidentListResponse` is a list wrapper (same precedent as `MemberListResponse`/`SloListResponse`), and `IncidentDetailResponse` is a join+aggregation response that duplicates most of `IncidentItem`'s fields plus several extra joined/optional fields and an embedded `timeline`. modelable has no "extends"/composition mechanism that would let `IncidentDetailResponse` reuse `Incident@1`'s fields without duplicating them, so modeling it as a third entity would mean hand-maintaining two near-identical field lists in `.mdl` — not worth it for a response type that's explicitly an aggregation, not a canonical domain entity. `IncidentDetailResponse` stays entirely hand-written, in both Rust and TypeScript, and keeps its own independent `resolved_at`/`triggered_by_rule_id`/etc. as `T | null` (unaffected by this migration).

### Fallout: `IncidentItem.resolved_at`/`triggered_by_rule_id`: `string | null` -> `?: string`

modelable emits optional fields as `field?: T` (i.e. `T | undefined`), not `T | null` — same wire-fidelity gap accepted in 3.4/3.5 for `Option<T>` fields that serialize to `null`. Unlike 3.5 (zero fallout), this domain has real fallout:

- `apps/frontend/src/features/incidents/IncidentsPage.tsx:48`: `items.filter((i) => i.status === "resolved" && i.resolved_at !== null)`. With `resolved_at?: string` (`string | undefined`), `undefined !== null` evaluates to `true`, so this filter would incorrectly match resolved incidents whose `resolved_at` is `undefined` too — but more importantly, it no longer correctly expresses "has a value". Fix: change `!== null` to `!= null` (loose inequality — `undefined != null` is `false`, `null != null` is `false`, any string `!= null` is `true`), which correctly means "is set" for both `null` and `undefined`.
- `apps/frontend/src/features/incidents/IncidentsPage.test.tsx`: fixtures at lines 31, 40 (`resolved_at: null`) and lines 32, 41, 50 (`triggered_by_rule_id: null`) become type errors against `?: string` (`null` is not assignable to `string | undefined`). Fix: omit these keys from the fixture objects entirely (cleaner than `: undefined`).
- `IncidentsPage.tsx:53` (`new Date(i.resolved_at!).getTime()`) and `IncidentsPage.tsx:162-163` (`{incident.resolved_at ? ... : ...}`) require no changes — non-null assertion and truthiness checks behave identically for `null` vs `undefined`.

`IncidentDetailResponse` is a separate, unaffected hand-written type — `IncidentDetailPage.tsx`'s `data.resolved_at`/`data.triggered_by_rule_id`/`data.runbook_url`/`data.impacted_service` (`!== null` / truthy checks) keep their current `T | null` types, no changes needed there.

### `IncidentEventItem.message`: `string | null` -> `?: string` — no fallout

`IncidentDetailPage.tsx:149` uses `event.message && (...)` (works identically for `null`/`undefined`). No test fixture sets `message: null` (`IncidentDetailPage.test.tsx` fixtures only set `message` to a string). Expected fallout: **none**.

## Goal

- Add `models/incidents.mdl` defining `incidents.Incident@1` (entity, mirrors `IncidentItem`) and `incidents.IncidentEvent@1` (event, mirrors `IncidentEventItem`).
- Generate and commit TypeScript artifacts (`apps/frontend/src/api/generated/incidents/`); re-export them as `IncidentItem`/`IncidentEventItem` from `apps/frontend/src/api/incidents.ts`.
- Fix the `resolved_at`/`triggered_by_rule_id` fallout in `IncidentsPage.tsx` (`!== null` -> `!= null`) and `IncidentsPage.test.tsx` (drop `resolved_at: null`/`triggered_by_rule_id: null` from fixtures).
- Add doc comments on `services/query-api/src/incidents.rs`'s `IncidentItem` and `IncidentEventItem` cross-referencing `models/incidents.mdl`. No Rust code changes beyond these comments.
- Mark Phase 3 step 3.6 done in the migration plan.

## Non-Goals

- `IncidentRow`, `IncidentEventRow`, `IncidentDetailRow`, `IncidentDetailResponse`, `IncidentListResponse` — all remain entirely hand-written (see Context/Scope).
- Any change to `services/query-api/src/incidents.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything needed (entity + event kinds + optional + timestamp + `@wire(json.fieldCase: ...)`, all used by prior phases).
- Recording a new Phase 1 backlog item — this migration hits only the already-recorded timestamp gap (item 5).

## Design

### 1. `models/incidents.mdl` (new file, new domain `incidents`)

```
domain incidents {
  owner: "platform-team"

  // Canonical incident summary entity. Mirrors
  // services/query-api/src/incidents.rs's IncidentItem field-for-field.
  // IncidentItem is the API response shape for the incidents list;
  // IncidentRow (Postgres sqlx::FromRow projection) and
  // IncidentDetailResponse (join + timeline aggregation) are NOT modeled
  // here — see
  // docs/superpowers/specs/2026-06-14-incidents-modelable-migration-design.md
  // for why (timestamp -> Rust String gap, Phase 1 backlog item 5).
  @wire(json.fieldCase: "snake_case")
  entity Incident @ 1 (additive) {
    @key incidentId: uuid
    title: string
    severity: string
    status: string
    triggeredAt: timestamp
    resolvedAt?: timestamp
    triggeredByRuleId?: uuid
  }

  // Canonical incident timeline event. Mirrors
  // services/query-api/src/incidents.rs's IncidentEventItem field-for-field.
  // Uses the `event` model kind (no @key — IncidentEventItem has no natural
  // unique field, and `event`/`value` kinds are the only ones that forbid
  // @key per modelable's semantic validation).
  @wire(json.fieldCase: "snake_case")
  event IncidentEvent @ 1 (additive) {
    eventTime: timestamp
    eventType: string
    actor: string
    message?: string
  }
}
```

Field-by-field, `Incident` mirrors `IncidentItem`: `incident_id` (UUID), `title`/`severity`/`status` (strings), `triggered_at` (`timestamp` -> TS `string`), `resolved_at`/`triggered_by_rule_id` (optional -> TS `string | undefined`). `IncidentEvent` mirrors `IncidentEventItem`: `event_time` (`timestamp` -> TS `string`), `event_type`/`actor` (strings), `message` (optional -> TS `string | undefined`).

No `@wire(json.case: ...)` needed — `severity`/`status`/`event_type` are plain `string` here (not `enum(...)`, since their value sets aren't fixed enums in the Rust code), so no case-mapping concern.

No `projection`/`binding` blocks — neither `IncidentRow`/`IncidentEventRow` (Postgres projections) nor `IncidentDetailResponse` (aggregation) are modeled (see Non-Goals).

### 2. Generated TypeScript artifacts

New directory `apps/frontend/src/api/generated/incidents/`, containing `incidents.Incident.v1.ts` and `incidents.IncidentEvent.v1.ts` (generated via `modelable compile` in an isolated scratch workspace — `incidents.mdl` has no `binding`/`projection` blocks, so it compiles standalone, same as `slos.mdl`/`admin.mdl`). Same regen-header-comment convention. Expected content:

`incidents.Incident.v1.ts`:
```typescript
export interface IncidentsIncidentV1 {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  triggered_at: string;
  resolved_at?: string;
  triggered_by_rule_id?: string;
}
export type Incident = IncidentsIncidentV1;
```

`incidents.IncidentEvent.v1.ts`:
```typescript
export interface IncidentsIncidentEventV1 {
  event_time: string;
  event_type: string;
  actor: string;
  message?: string;
}
export type IncidentEvent = IncidentsIncidentEventV1;
```

In `apps/frontend/src/api/incidents.ts`, replace the hand-written `IncidentItem` interface (current lines 5-13) and `IncidentEventItem` interface (current lines 19-24) with:

```typescript
import type { Incident as IncidentItem } from "./generated/incidents/incidents.Incident.v1";
export type { IncidentItem };

import type { IncidentEvent as IncidentEventItem } from "./generated/incidents/incidents.IncidentEvent.v1";
export type { IncidentEventItem };
```

`IncidentListResponse`, `IncidentDetailResponse`, `listIncidents`, `getIncident` are unchanged (the former two now reference the generated types transitively via `IncidentItem`/`IncidentEventItem`).

### 3. Type-fallout fixes

- `apps/frontend/src/features/incidents/IncidentsPage.tsx:48`: change `i.resolved_at !== null` to `i.resolved_at != null`.
- `apps/frontend/src/features/incidents/IncidentsPage.test.tsx`: remove `resolved_at: null` (lines 31, 40) and `triggered_by_rule_id: null` (lines 32, 41, 50) from the `sampleIncidents` fixture objects (omit the keys rather than setting `undefined`).
- No other files require changes — see Context's fallout analysis for `IncidentDetailResponse`/`IncidentEventItem.message` (both unaffected).

### 4. Rust lineage comments

In `services/query-api/src/incidents.rs`, add doc comments above `IncidentItem` and `IncidentEventItem`:

```rust
/// Canonical incident summary entity. Mirrors `incidents.Incident@1` in
/// `models/incidents.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-incidents-modelable-migration-design.md`).
/// `IncidentRow` (the Postgres `sqlx::FromRow` projection) and
/// `IncidentDetailResponse` (join + timeline aggregation) are NOT modeled —
/// timestamp fields stay `chrono::DateTime<Utc>` (Phase 1 backlog item 5:
/// modelable's `timestamp` emits as Rust `String`).
#[derive(Serialize)]
pub struct IncidentItem {
```

```rust
/// Canonical incident timeline event. Mirrors `incidents.IncidentEvent@1` in
/// `models/incidents.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-incidents-modelable-migration-design.md`).
/// `event_time` stays `chrono::DateTime<Utc>` (Phase 1 backlog item 5).
#[derive(Serialize)]
pub struct IncidentEventItem {
```

No other Rust changes — `IncidentRow`, `IncidentDetailRow`, `IncidentEventRow`, `IncidentListResponse`, `IncidentDetailResponse`, and all handlers/functions are untouched.

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes (`OK 7 files valid.`).
- Lineage proof: `modelable lineage incidents.Incident@1` and `modelable lineage incidents.IncidentEvent@1`, included in the commit/PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build`.
- Rust: `cargo fmt --all && cargo check` for `query-api` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.6 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
