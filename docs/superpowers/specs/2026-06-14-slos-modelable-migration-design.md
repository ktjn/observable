# SLOs Domain: Modelable Migration Design

**Date:** 2026-06-14
**Status:** Approved
**Scope:** Phase 3 step 3.5 (SLOs portion) of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/slos.mdl` for the `SloDefinition` entity, generate and commit a TypeScript artifact, and wire it into `apps/frontend/src/api/slos.ts`. Document why the migration is TS-only (reusing Phase 1 backlog item 5).

## Context

`services/query-api/src/slos.rs` defines:

- `SloDefinitionItem` (`#[derive(Debug, Clone, Serialize, sqlx::FromRow)]`): `slo_id: Uuid`, `service_name: String`, `environment: String`, `sli_type: String` (always `"availability"` — the only SLI type currently implemented), `target: f64`, `window_days: i32`, `burn_rate_fast_threshold: f64`, `burn_rate_slow_threshold: f64`, `description: String`, `firing: bool`, `last_fired_at: Option<DateTime<Utc>>`, `created_at: DateTime<Utc>`, `updated_at: DateTime<Utc>`. This single struct serves as **both** the Postgres row (via `sqlx::FromRow`, joining `slo_definitions` with `alert_rules`/`alert_firings` to compute `firing`/`last_fired_at`) **and** the API response shape returned by `handle_list_slos`/`handle_create_slo` — there is no separate row/canonical split, same as `admin.Member` (3.4).
- `SloListResponse { items: Vec<SloDefinitionItem> }` — list wrapper.
- `CreateSloRequest { service_name, environment, target, window_days, burn_rate_fast_threshold, burn_rate_slow_threshold, description }` — request body.

`apps/frontend/src/api/slos.ts` hand-writes a matching `SloDefinitionItem` interface (`sli_type: "availability"`, `last_fired_at: string | null`), `SloListResponse`, and `CreateSloRequest`.

### Why TS-only ("Option B") — reusing Phase 1 backlog item 5

`target`/`burn_rate_fast_threshold`/`burn_rate_slow_threshold` (modelable `float` → Rust `f64`) and `window_days` (`int` with `@wire(rust.type: "i32")` → Rust `i32`) match the existing Rust types exactly — no regression. `sli_type: enum(availability)` → Rust `String` matches the current `sli_type: String` — no regression (same as `admin.Member.role`, 3.4).

However, `last_fired_at: Option<DateTime<Utc>>`, `created_at: DateTime<Utc>`, and `updated_at: DateTime<Utc>` are required for `sqlx::FromRow` to map Postgres `TIMESTAMPTZ` columns. modelable's `timestamp` primitive emits as Rust `String` (`_primitive_to_rust` in `cli/src/modelable/emitters/rust.py`), with no `@wire(rust.type: ...)` override available for non-`int` fields (`cli/src/modelable/validation/semantic.py:538`). Generating `SloDefinitionItem` in Rust would replace `chrono::DateTime<Utc>`/`Option<DateTime<Utc>>` with `String`, breaking the `sqlx::FromRow` mapping — a regression.

So, same as 3.2-3.4, this migration is **TS-only**: generate the TypeScript artifact, re-export it into `slos.ts`, and add a lineage doc comment to the Rust `SloDefinitionItem` without changing its code. This is the **same** gap as Phase 1 backlog item 5 (recorded in 3.4) — no new backlog item is needed.

### `last_fired_at`: `string | null` -> TS `last_fired_at?: string`

The hand-written `SloDefinitionItem.last_fired_at: string | null` reflects that `Option<DateTime<Utc>>` serializes to `null` (serde's default), not an omitted field. modelable emits optional fields as `last_fired_at?: string` (i.e. `string | undefined`), same as `name?: string` in 3.4. This is a minor, pre-accepted wire-fidelity gap.

Checked for fallout: `apps/frontend/src/features/alerts/AlertsPage.tsx`'s `SloHealthCard` and `apps/frontend/src/api/reliability.ts` (which re-exports `SloDefinitionItem` via `slos: SloDefinitionItem[]`) do not read `.last_fired_at` at all. All `last_fired_at: null` occurrences found via grep belong to the unrelated `AlertRuleItem`/`AlertRuleSummary` type (`apps/frontend/src/api/alerts.ts`), not `SloDefinitionItem`. Expected fallout: **none**.

### `SloListResponse` / `CreateSloRequest` stay hand-written

Per the Phase 3 "per-domain rule" (handler-local request/wrapper shapes may stay hand-written): `SloListResponse` is a list wrapper (same precedent as `MemberListResponse`/`LogListResponse`-style types), and `CreateSloRequest` is a request body (same precedent as `CreateChannelRequest`/`AddMemberRequest`). Both are unchanged by this migration, in both Rust and TypeScript.

## Goal

- Add `models/slos.mdl` defining `slos.SloDefinition@1` (canonical entity), mirroring `SloDefinitionItem` field-for-field.
- Generate and commit a TypeScript artifact (`apps/frontend/src/api/generated/slos/`); re-export it as `SloDefinitionItem` from `apps/frontend/src/api/slos.ts`.
- Add a doc comment on `services/query-api/src/slos.rs`'s `SloDefinitionItem` cross-referencing `models/slos.mdl` (`SloDefinition@1`) for lineage tracking. No Rust code changes beyond this comment.
- Mark the SLOs portion of Phase 3 step 3.5 done in the migration plan.

## Non-Goals

- `SloListResponse`, `CreateSloRequest` — both remain hand-written (see Context).
- `SchemaEntry`/`SemanticAnnotation` (`services/query-api/src/schemas.rs`) — deferred. These have no frontend TS consumer (confirmed via grep), so generating TS artifacts for them now would create unused code (YAGNI). Schemas may be revisited in a future step once/if the frontend needs them.
- Any change to `services/query-api/src/slos.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything `SloDefinition@1` needs (entity + enum + optional + timestamp + float + `@wire(rust.type: ...)` + `@wire(json.fieldCase: ...)`, all used by prior phases).
- Recording a new Phase 1 backlog item — this migration hits only the already-recorded timestamp gap (item 5).

## Design

### 1. `models/slos.mdl` (new file, new domain `slos`)

```
domain slos {
  owner: "platform-team"

  // Canonical SLO definition entity. Mirrors
  // services/query-api/src/slos.rs's SloDefinitionItem field-for-field.
  // SloDefinitionItem serves as both the Postgres db row (sqlx::FromRow,
  // joined with alert_rules/alert_firings for firing/last_fired_at) and the
  // API response shape; it is NOT modeled in Rust here — see
  // docs/superpowers/specs/2026-06-14-slos-modelable-migration-design.md
  // for why (timestamp -> Rust String gap, Phase 1 backlog item 5).
  @wire(json.fieldCase: "snake_case")
  entity SloDefinition @ 1 (additive) {
    @key sloId: uuid
    serviceName: string
    environment: string
    sliType: enum(availability)
    target: float
    @wire(rust.type: "i32")
    windowDays: int
    burnRateFastThreshold: float
    burnRateSlowThreshold: float
    description: string
    firing: bool
    lastFiredAt?: timestamp
    createdAt: timestamp
    updatedAt: timestamp
  }
}
```

Field-by-field, this mirrors `SloDefinitionItem`: `slo_id` (UUID), `service_name`/`environment`/`description` (strings), `sli_type` (`enum(availability)` -> TS literal `"availability"`, matching the hand-written type exactly), `target`/`burn_rate_fast_threshold`/`burn_rate_slow_threshold` (`float` -> TS `number`), `window_days` (`int` with `@wire(rust.type: "i32")` -> TS `number`), `firing` (`bool` -> TS `boolean`), `last_fired_at` (optional `timestamp` -> TS `string | undefined`), `created_at`/`updated_at` (`timestamp` -> TS `string`).

No `@wire(json.case: ...)` is needed on `sliType` — `enum(...)` with no case hint emits values as written, already lowercase (`availability`), matching both the Rust `sli_type: String` wire value and the hand-written TS literal.

No `projection`/`binding` blocks — `SloDefinitionItem` (the combined db-row/response struct) is not modeled in Rust (see Non-Goals).

### 2. Generated TypeScript artifact

New directory `apps/frontend/src/api/generated/slos/`, containing `slos.SloDefinition.v1.ts` (generated via `modelable compile` in an isolated scratch workspace — `slos.mdl` has no `binding`/`projection` blocks, so it compiles standalone, same as `admin.mdl`/`notifications.mdl`). Same regen-header-comment convention. Expected content:

```typescript
export interface SlosSloDefinitionV1 {
  slo_id: string;
  service_name: string;
  environment: string;
  sli_type: 'availability';
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description: string;
  firing: boolean;
  last_fired_at?: string;
  created_at: string;
  updated_at: string;
}
export type SloDefinition = SlosSloDefinitionV1;
```

In `apps/frontend/src/api/slos.ts`, replace the hand-written `SloDefinitionItem` interface (current lines 5-19) with:

```typescript
import type { SloDefinition as SloDefinitionItem } from "./generated/slos/slos.SloDefinition.v1";
export type { SloDefinitionItem };
```

`SloListResponse`, `CreateSloRequest`, `listSlos`, `createSlo` are unchanged.

### 3. Type-fallout check

`grep` of `SloDefinitionItem` usage across the frontend (`apps/frontend/src/api/reliability.ts`, `apps/frontend/src/features/alerts/AlertsPage.tsx`'s `SloHealthCard`, and test fixtures) confirms no code reads `.last_fired_at`, and no `SloDefinitionItem`-typed literal sets `last_fired_at: null` (the `null` occurrences found belong to the unrelated `AlertRuleItem`/`AlertRuleSummary` type). Expected fallout: **none**.

### 4. Rust lineage comment

In `services/query-api/src/slos.rs`, add a doc comment above `SloDefinitionItem`:

```rust
/// Canonical SLO definition entity. Mirrors `slos.SloDefinition@1` in
/// `models/slos.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-slos-modelable-migration-design.md`).
/// Also serves as the Postgres `sqlx::FromRow` projection joining
/// `slo_definitions` with `alert_rules`/`alert_firings` for `firing`/
/// `last_fired_at` — timestamp fields stay `chrono::DateTime<Utc>`
/// (Phase 1 backlog item 5: modelable's `timestamp` emits as Rust `String`).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SloDefinitionItem {
```

No other Rust changes — `CreateSloRequest`, `SloListResponse`, and all handlers/functions are untouched.

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes (`OK 6 files valid.`).
- Lineage proof: `modelable lineage slos.SloDefinition@1`, included in the commit/PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build` from `apps/frontend/`.
- Rust: `cargo fmt --all && cargo check` for `query-api` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh`.
- Mark the SLOs portion of Phase 3 step 3.5 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, and note the Schemas deferral.
