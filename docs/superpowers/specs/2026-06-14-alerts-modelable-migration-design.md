# Alerts Domain: Modelable Migration Design

**Date:** 2026-06-14
**Status:** Approved
**Scope:** Phase 3 step 3.7 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/alerts.mdl` for the `AlertRuleItem` and `FiringItem` shapes, generate and commit TypeScript artifacts, and wire them into `apps/frontend/src/api/alerts.ts`. Document why `AlertRuleListResponse`/`AlertRuleDetailResponse`/`AlertRuleRow`/`AlertRuleDetailRow`/`FiringRow`/`CreateRuleRequest`/`SilenceRequest`/`UpdateRunbookRequest` remain hand-written, and fix the resulting type-fallout in `AlertRuleDetailPage.tsx`/`AlertRuleDetailPage.test.tsx`/`ServiceAlertsTab.test.tsx`.

## Context

`services/query-api/src/alerts.rs` defines several shapes:

- `AlertRuleItem` (`#[derive(Serialize, Clone, Debug, PartialEq)]`, lines 14-28): `rule_id: Uuid`, `name`/`metric_name`/`operator`/`severity`/`state: String`, `threshold: f64`, `silenced`/`firing: bool`, `last_fired_at: Option<DateTime<Utc>>`, `notification_channels: Vec<Uuid>`, `auto_trigger_incident: bool`. This is the API response shape for the alert rules list (`list_alert_rules`, also returned from `create_alert_rule`/`silence_alert_rule`). The Postgres row (`AlertRuleRow`, `#[derive(sqlx::FromRow)]`) is a separate hand-written struct.
- `FiringItem` (`#[derive(Serialize)]`, lines 35-42): `firing_id: Uuid`, `state: String`, `value: Option<f64>`, `occurred_at: DateTime<Utc>`, `resolved_at: Option<DateTime<Utc>>`. Embedded as `AlertRuleDetailResponse.firings: Vec<FiringItem>`. Likewise has a separate hand-written row (`FiringRow`, `#[derive(sqlx::FromRow)]`).
- `AlertRuleListResponse { items: Vec<AlertRuleItem> }` — list wrapper.
- `AlertRuleDetailResponse` (lines 44-55): a join+aggregation response from `get_alert_rule` — `rule_id`, `name`, `severity`, `alert_type`, `condition: serde_json::Value`, `silenced`, `firing`, `firings: Vec<FiringItem>`, `runbook_url: Option<String>`. Backed by `AlertRuleDetailRow` (`#[derive(sqlx::FromRow)]`, a join of `alert_rules` plus an `EXISTS` subquery against `alert_firings`).
- `CreateRuleRequest`, `SilenceRequest`, `UpdateRunbookRequest` (`#[derive(Deserialize)]`) — request bodies.

`apps/frontend/src/api/alerts.ts` hand-writes matching `AlertRuleItem` (lines 5-18), `AlertRuleListResponse`, `CreateRuleRequest`, `FiringItem` (lines 69-75), and `AlertRuleDetailResponse` (lines 77-87) interfaces (`last_fired_at`/`value`/`resolved_at`/`runbook_url` all as `T | null`).

### Why TS-only ("Option B") — reusing Phase 1 backlog item 5

`operator`/`severity`/`state` are plain `String` in Rust. `last_fired_at`/`occurred_at`/`resolved_at` are `Option<DateTime<Utc>>`/`DateTime<Utc>`, required for `sqlx::FromRow` row-mapping in `AlertRuleRow`/`FiringRow`/`AlertRuleDetailRow` (not modeled here, but `AlertRuleItem`/`FiringItem` are built field-by-field from those rows, so generating them in Rust would still force a `String`-vs-`DateTime<Utc>` mismatch at the conversion site). modelable's `timestamp` primitive emits as Rust `String` (Phase 1 backlog item 5). So, same as 3.2-3.6, this migration is **TS-only**: generate TypeScript artifacts, re-export them into `alerts.ts`, and add lineage doc comments to the Rust `AlertRuleItem`/`FiringItem` without changing their code. No new backlog item needed.

### Scope: `AlertRuleItem`/`FiringItem` only

Per the "per-domain rule" (handler-local request/aggregation shapes may stay hand-written): `AlertRuleListResponse` is a list wrapper (same precedent as `MemberListResponse`/`SloListResponse`/`IncidentListResponse`), and `AlertRuleDetailResponse` is a join+aggregation response that duplicates several of `AlertRuleItem`'s fields (`rule_id`, `name`, `severity`, `silenced`, `firing`) plus extra joined/optional fields (`alert_type`, `condition`, `runbook_url`) and an embedded `firings: Vec<FiringItem>`. modelable has no "extends"/composition mechanism that would let `AlertRuleDetailResponse` reuse `AlertRuleItem`'s fields without duplicating them, so modeling it as a third entity would mean hand-maintaining two near-identical field lists in `.mdl` — not worth it for a response type that's explicitly an aggregation, not a canonical domain entity (same reasoning as 3.6's `IncidentDetailResponse`). `AlertRuleDetailResponse` stays entirely hand-written, in both Rust and TypeScript, but its `firings: FiringItem[]` field references the now-generated `FiringItem` type.

`AlertRuleItem` and `FiringItem` both have natural unique keys (`rule_id`, `firing_id`), so both are modeled as `entity` kind (unlike 3.6's `IncidentEvent`, which used `event` kind for lacking a natural key).

### `operator`/`state` as `enum(...)`

- `operator: enum(gt, gte, lt, lte, eq)` — matches `VALID_OPERATORS` (alerts.rs:12), a real Rust-side constraint validated in `create_alert_rule` (alerts.rs:217-222). Same precedent as `slos.mdl`'s `sliType: enum(availability)` and `admin.mdl`'s `role: enum(tenant_admin, member, viewer)`.
- `AlertRuleItem.state: enum(ok, pending, active, resolved, silenced)` — matches the SQL that produces this column (alerts.rs:150-159): `CASE WHEN r.silenced THEN 'silenced' ELSE COALESCE((SELECT af.state FROM alert_firings af ... ORDER BY CASE WHEN af.state IN ('pending', 'active') THEN 0 ELSE 1 END, af.occurred_at DESC LIMIT 1), 'ok') END AS state`. The `'silenced'` branch confirms the hand-written TS union (`"ok" | "pending" | "active" | "resolved" | "silenced"`) is accurate, not aspirational.
- `FiringItem.state: enum(pending, active, resolved)` — matches the existing hand-written TS union and the `alert_firings.state` values referenced across `alerts.rs`/`slos.rs`/`reliability.rs` (`'active'`, `'pending'`, and `'resolved'` implied by the `resolved_at` column).

### Fallout: `Option<T>` fields → `?: T`

modelable emits optional fields as `field?: T` (i.e. `T | undefined`), not `T | null` — same wire-fidelity gap accepted in 3.4-3.6 for `Option<T>` fields that serialize to `null`.

- `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx:180`: `{firing.value !== null ? firing.value.toFixed(2) : "—"}`. With `value?: number` (`number | undefined`), `undefined !== null` evaluates to `true`, so `firing.value.toFixed(2)` would be called on `undefined`, throwing at runtime. Fix: change `!== null` to `!= null` (loose inequality — `undefined != null` and `null != null` are both `false`, any number `!= null` is `true`), same pattern as 3.6's `IncidentsPage.tsx:48`.
- `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx:37`: the `f-1` firing fixture sets `resolved_at: null,`. With `resolved_at?: string`, `null` is not assignable to `string | undefined` — a type error. Fix: remove the `resolved_at: null,` line entirely (omit the key, same as 3.6's fixture fixes).
- `apps/frontend/src/features/services/ServiceAlertsTab.test.tsx:54`: the `okRule: alertsApi.AlertRuleItem` fixture sets `last_fired_at: null,`. With `last_fired_at?: string`, this is a type error. Fix: remove the `last_fired_at: null,` line entirely.
- `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx:186-188` (`{firing.resolved_at ? formatTimestamp(...) : "—"}`) and `apps/frontend/src/features/services/ServiceAlertsTab.tsx:82-88` (`{rule.last_fired_at ? formatTimestamp(...) : "—"}`) require no changes — truthiness checks behave identically for `null` vs `undefined`.
- `apps/frontend/src/App.test.tsx` raw `fetch` mock responses (lines 1346, 1384, etc., `last_fired_at: "..."` / `last_fired_at: null` inside `JSON.stringify({...})`) are untyped object literals passed to `JSON.stringify` — no type error, no change needed.

## Goal

- Add `models/alerts.mdl` defining `alerts.AlertRule@1` (entity, mirrors `AlertRuleItem`) and `alerts.Firing@1` (entity, mirrors `FiringItem`).
- Generate and commit TypeScript artifacts (`apps/frontend/src/api/generated/alerts/`); re-export them as `AlertRuleItem`/`FiringItem` from `apps/frontend/src/api/alerts.ts`.
- Fix the `value`/`resolved_at`/`last_fired_at` fallout in `AlertRuleDetailPage.tsx` (`!== null` → `!= null`), `AlertRuleDetailPage.test.tsx` (drop `resolved_at: null`), and `ServiceAlertsTab.test.tsx` (drop `last_fired_at: null`).
- Add doc comments on `services/query-api/src/alerts.rs`'s `AlertRuleItem` and `FiringItem` cross-referencing `models/alerts.mdl`. No Rust code changes beyond these comments.
- Mark Phase 3 step 3.7 done in the migration plan.

## Non-Goals

- `AlertRuleRow`, `FiringRow`, `AlertRuleDetailRow`, `AlertRuleListResponse`, `AlertRuleDetailResponse`, `CreateRuleRequest`, `SilenceRequest`, `UpdateRunbookRequest` — all remain entirely hand-written (see Context/Scope).
- Any change to `services/query-api/src/alerts.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything needed (entity kind + optional + timestamp + enum + array<uuid> + `@wire(json.fieldCase: ...)`, all used by prior phases).
- Recording a new Phase 1 backlog item — this migration hits only the already-recorded timestamp gap (item 5).

## Design

### 1. `models/alerts.mdl` (new file, new domain `alerts`)

```
domain alerts {
  owner: "platform-team"

  // Canonical alert rule summary entity. Mirrors
  // services/query-api/src/alerts.rs's AlertRuleItem field-for-field.
  // AlertRuleListResponse (list wrapper), AlertRuleDetailResponse (join +
  // firings aggregation), AlertRuleRow/AlertRuleDetailRow (Postgres
  // sqlx::FromRow projections), and CreateRuleRequest/SilenceRequest/
  // UpdateRunbookRequest (request bodies) are NOT modeled here — see
  // docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md
  // for why (timestamp -> Rust String gap, Phase 1 backlog item 5).
  @wire(json.fieldCase: "snake_case")
  entity AlertRule @ 1 (additive) {
    @key ruleId: uuid
    name: string
    metricName: string
    operator: enum(gt, gte, lt, lte, eq)
    threshold: float
    severity: string
    silenced: bool
    state: enum(ok, pending, active, resolved, silenced)
    firing: bool
    lastFiredAt?: timestamp
    notificationChannels: array<uuid>
    autoTriggerIncident: bool
  }

  // Canonical alert firing entity. Mirrors
  // services/query-api/src/alerts.rs's FiringItem field-for-field. Embedded
  // as AlertRuleDetailResponse.firings (hand-written, see above).
  @wire(json.fieldCase: "snake_case")
  entity Firing @ 1 (additive) {
    @key firingId: uuid
    state: enum(pending, active, resolved)
    value?: float
    occurredAt: timestamp
    resolvedAt?: timestamp
  }
}
```

Field-by-field, `AlertRule` mirrors `AlertRuleItem`: `rule_id` (UUID), `name`/`metric_name` (strings), `operator` (enum → TS `"gt" | "gte" | "lt" | "lte" | "eq"`), `threshold` (float), `severity` (string), `silenced`/`firing` (bools), `state` (enum → TS `"ok" | "pending" | "active" | "resolved" | "silenced"`), `last_fired_at` (optional `timestamp` → TS `string | undefined`), `notification_channels` (`array<uuid>` → TS `string[]`), `auto_trigger_incident` (bool). `Firing` mirrors `FiringItem`: `firing_id` (UUID), `state` (enum → TS `"pending" | "active" | "resolved"`), `value` (optional float → TS `number | undefined`), `occurred_at` (`timestamp` → TS `string`), `resolved_at` (optional `timestamp` → TS `string | undefined`).

No `projection`/`binding` blocks — neither `AlertRuleRow`/`FiringRow`/`AlertRuleDetailRow` (Postgres projections) nor `AlertRuleListResponse`/`AlertRuleDetailResponse` (list wrapper / aggregation) are modeled (see Non-Goals).

### 2. Generated TypeScript artifacts

New directory `apps/frontend/src/api/generated/alerts/`, containing `alerts.AlertRule.v1.ts` and `alerts.Firing.v1.ts` (generated via `modelable compile` in an isolated scratch workspace — `alerts.mdl` has no `binding`/`projection` blocks, so it compiles standalone, same as `incidents.mdl`/`slos.mdl`/`admin.mdl`). Same regen-header-comment convention. Expected content:

`alerts.AlertRule.v1.ts`:
```typescript
export interface AlertsAlertRuleV1 {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: "gt" | "gte" | "lt" | "lte" | "eq";
  threshold: number;
  severity: string;
  silenced: boolean;
  state: "ok" | "pending" | "active" | "resolved" | "silenced";
  firing: boolean;
  last_fired_at?: string;
  notification_channels: string[];
  auto_trigger_incident: boolean;
}
export type AlertRule = AlertsAlertRuleV1;
```

`alerts.Firing.v1.ts`:
```typescript
export interface AlertsFiringV1 {
  firing_id: string;
  state: "pending" | "active" | "resolved";
  value?: number;
  occurred_at: string;
  resolved_at?: string;
}
export type Firing = AlertsFiringV1;
```

In `apps/frontend/src/api/alerts.ts`, replace the hand-written `AlertRuleItem` interface (current lines 5-18) and `FiringItem` interface (current lines 69-75) with:

```typescript
import type { AlertRule as AlertRuleItem } from "./generated/alerts/alerts.AlertRule.v1";
export type { AlertRuleItem };

import type { Firing as FiringItem } from "./generated/alerts/alerts.Firing.v1";
export type { FiringItem };
```

`AlertRuleListResponse`, `AlertRuleDetailResponse`, `CreateRuleRequest`, `listAlertRules`, `createAlertRule`, `silenceAlertRule`, `getAlertRule`, `setAlertRuleRunbook` are unchanged (`AlertRuleListResponse`/`AlertRuleDetailResponse` now reference the generated types transitively via `AlertRuleItem`/`FiringItem`).

### 3. Type-fallout fixes

- `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx:180`: change `firing.value !== null ? firing.value.toFixed(2) : "—"` to `firing.value != null ? firing.value.toFixed(2) : "—"`.
- `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`: remove `resolved_at: null,` (line 37) from the `f-1` firing fixture.
- `apps/frontend/src/features/services/ServiceAlertsTab.test.tsx`: remove `last_fired_at: null,` (line 54) from the `okRule` fixture.
- No other files require changes — see Context's fallout analysis for `AlertRuleDetailPage.tsx:186-188`, `ServiceAlertsTab.tsx:82-88`, and `App.test.tsx` (all unaffected).

### 4. Rust lineage comments

In `services/query-api/src/alerts.rs`, add doc comments above `AlertRuleItem` and `FiringItem`:

```rust
/// Canonical alert rule summary entity. Mirrors `alerts.AlertRule@1` in
/// `models/alerts.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`).
/// `AlertRuleRow`/`AlertRuleDetailRow` (Postgres `sqlx::FromRow`
/// projections) and `AlertRuleListResponse`/`AlertRuleDetailResponse` (list
/// wrapper / join+firings aggregation) are NOT modeled — timestamp fields
/// stay `chrono::DateTime<Utc>` (Phase 1 backlog item 5: modelable's
/// `timestamp` emits as Rust `String`).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AlertRuleItem {
```

```rust
/// Canonical alert firing entity. Mirrors `alerts.Firing@1` in
/// `models/alerts.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`).
/// `occurred_at`/`resolved_at` stay `chrono::DateTime<Utc>` (Phase 1
/// backlog item 5).
#[derive(Serialize)]
pub struct FiringItem {
```

No other Rust changes — `AlertRuleRow`, `AlertRuleDetailRow`, `FiringRow`, `AlertRuleListResponse`, `AlertRuleDetailResponse`, and all handlers/functions are untouched.

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes (`OK 8 files valid.`).
- Lineage proof: `modelable lineage alerts.AlertRule@1` and `modelable lineage alerts.Firing@1`, included in the commit/PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build`.
- Rust: `cargo fmt --all && cargo check` for `query-api` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.7 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
