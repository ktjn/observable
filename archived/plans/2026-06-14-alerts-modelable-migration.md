# Alerts Domain: Modelable Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `models/alerts.mdl` defining `alerts.AlertRule@1` and `alerts.Firing@1`, generate their TypeScript artifacts, re-export them as `AlertRuleItem`/`FiringItem` from `apps/frontend/src/api/alerts.ts`, fix the resulting `value`/`resolved_at`/`last_fired_at` type-fallout in `AlertRuleDetailPage.tsx`/`AlertRuleDetailPage.test.tsx`/`ServiceAlertsTab.test.tsx`, and add lineage doc comments to `AlertRuleItem`/`FiringItem` in `services/query-api/src/alerts.rs`. Mark Phase 3 step 3.7 done.

**Architecture:** This is a TS-only migration (Phase 3 step 3.7, scope "AlertRuleItem + FiringItem" from the approved design), reusing the already-recorded Phase 1 backlog item 5 (modelable's `timestamp` primitive emits as Rust `String`, incompatible with `sqlx::FromRow`'s `chrono::DateTime<Utc>`). `AlertRuleListResponse` and `AlertRuleDetailResponse` stay hand-written — see `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md` for the full rationale.

**Tech Stack:** modelable 0.4.0 (pinned in `models/requirements.txt`, no bump needed), TypeScript/Vite/Vitest frontend, Rust/Axum/sqlx backend.

---

## Task 1: Author `models/alerts.mdl`

**Files:**
- Create: `models/alerts.mdl`

- [ ] **Step 1: Create `models/alerts.mdl`**

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

- [ ] **Step 2: Validate**

This validates the whole `models/` workspace (`logs.mdl`, `tracing.mdl`, `metrics.mdl`, `notifications.mdl`, `admin.mdl`, `slos.mdl`, `incidents.mdl`, `alerts.mdl`, `requirements.txt`):

```bash
cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models
```

Expected: `OK 8 files valid.` (confirms `alerts.mdl` parses and has no semantic errors alongside the existing files).

- [ ] **Step 3: Commit**

```bash
git add models/alerts.mdl
git commit -m "feat(models): author alerts.mdl (AlertRule, Firing)"
```

---

## Task 2: Generate and wire TypeScript `AlertRuleItem`/`FiringItem`

**Files:**
- Create: `apps/frontend/src/api/generated/alerts/alerts.AlertRule.v1.ts`
- Create: `apps/frontend/src/api/generated/alerts/alerts.Firing.v1.ts`
- Modify: `apps/frontend/src/api/alerts.ts:1-18`, `apps/frontend/src/api/alerts.ts:69-75`

- [ ] **Step 1: Generate the TypeScript artifacts**

`alerts.mdl` has no `projection`/`binding` blocks, so it compiles standalone without hitting the duplicate-`binding`-name issue (Phase 1 backlog item 4). Generate it in an isolated scratch workspace:

```bash
mkdir -p /c/tmp/alerts-gen/models
cp /c/git/Observable/models/alerts.mdl /c/git/Observable/models/requirements.txt /c/tmp/alerts-gen/models/
cd /c/tmp/alerts-gen && /c/git/modelable/cli/.venv/Scripts/python.exe -m modelable compile models --target typescript --out ts
```

Expected: `OK C:\tmp\alerts-gen\ts\alerts.AlertRule.v1.ts` and `OK C:\tmp\alerts-gen\ts\alerts.Firing.v1.ts`, each with a content hash line. If `notificationChannels: array<uuid>` or either `enum(...)` field fails to compile, read the error, consult `C:\git\modelable\cli\src\modelable\emitters\typescript.py` for the supported syntax, and adjust `models/alerts.mdl` (back in Task 1) accordingly before proceeding — re-run Task 1 Step 2 (`validate`) after any change.

- [ ] **Step 2: Create `apps/frontend/src/api/generated/alerts/alerts.AlertRule.v1.ts`**

Copy the generated file verbatim, prefixed with the same regen-header convention used by `apps/frontend/src/api/generated/incidents/incidents.Incident.v1.ts`:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/alerts.mdl and models/requirements.txt into
// an isolated scratch workspace (alerts.mdl has no bindings/projections,
// so it compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy alerts.AlertRule.v1.ts into this directory.
/**
 * @modelable domain: alerts
 * @modelable name: AlertRule
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface AlertsAlertRuleV1 {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: 'gt' | 'gte' | 'lt' | 'lte' | 'eq';
  threshold: number;
  severity: string;
  silenced: boolean;
  state: 'ok' | 'pending' | 'active' | 'resolved' | 'silenced';
  firing: boolean;
  last_fired_at?: string;
  notification_channels: string[];
  auto_trigger_incident: boolean;
}
export type AlertRule = AlertsAlertRuleV1;
```

If the actual generated output differs from the body shown above (field order, optional-field syntax, quote style, etc.), use the actual generated output instead — only the header comment block is fixed by convention.

- [ ] **Step 3: Create `apps/frontend/src/api/generated/alerts/alerts.Firing.v1.ts`**

Same regen-header convention:

```typescript
// Generated by modelable — do not edit by hand.
// Regenerate: copy models/alerts.mdl and models/requirements.txt into
// an isolated scratch workspace (alerts.mdl has no bindings/projections,
// so it compiles standalone — see Phase 1 backlog item 4 for why the full
// C:\git\Observable\models directory currently cannot be compiled from
// scratch), then run:
//   cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable compile <scratch-workspace>\models --target typescript --out <scratch-out>
// then copy alerts.Firing.v1.ts into this directory.
/**
 * @modelable domain: alerts
 * @modelable name: Firing
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface AlertsFiringV1 {
  firing_id: string;
  state: 'pending' | 'active' | 'resolved';
  value?: number;
  occurred_at: string;
  resolved_at?: string;
}
export type Firing = AlertsFiringV1;
```

If the actual generated output differs from the body shown above, use the actual generated output instead — only the header comment block is fixed by convention.

- [ ] **Step 4: Wire the re-exports into `apps/frontend/src/api/alerts.ts`**

Current `apps/frontend/src/api/alerts.ts:1-18`:

```typescript
function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export interface AlertRuleItem {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: "gt" | "gte" | "lt" | "lte" | "eq";
  threshold: number;
  severity: string;
  silenced: boolean;
  state: "ok" | "pending" | "active" | "resolved" | "silenced";
  firing: boolean;
  last_fired_at: string | null;
  notification_channels: string[];
  auto_trigger_incident: boolean;
}
```

Replace with:

```typescript
import type { AlertRule as AlertRuleItem } from "./generated/alerts/alerts.AlertRule.v1";
import type { Firing as FiringItem } from "./generated/alerts/alerts.Firing.v1";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type { AlertRuleItem };
```

Current `apps/frontend/src/api/alerts.ts:69-75`:

```typescript
export interface FiringItem {
  firing_id: string;
  state: "pending" | "active" | "resolved";
  value: number | null;
  occurred_at: string;
  resolved_at: string | null;
}
```

Replace with:

```typescript
export type { FiringItem };
```

The two `import type` statements are placed at the very top of `apps/frontend/src/api/alerts.ts`, before `tenantHeaders` (per standard TypeScript import placement, same convention as 3.6's `incidents.ts`), while the `export type { ... }` statements stay at their original locations (now single-line each).

The rest of `alerts.ts` (`AlertRuleListResponse`, `CreateRuleRequest`, `AlertRuleDetailResponse`, `listAlertRules`, `createAlertRule`, `silenceAlertRule`, `getAlertRule`, `setAlertRuleRunbook`) is unchanged — `AlertRuleListResponse.items: AlertRuleItem[]` and `AlertRuleDetailResponse.firings: FiringItem[]` now reference the generated types transitively.

- [ ] **Step 5: Typecheck**

```bash
cd apps/frontend && npm run typecheck
```

Expected: errors in `AlertRuleDetailPage.tsx`, `AlertRuleDetailPage.test.tsx`, and `ServiceAlertsTab.test.tsx` due to `value`/`resolved_at`/`last_fired_at` changing from `T | null` to `?: T` — these are fixed in Task 3. Do not fix them in this task; just confirm the errors are limited to those three files (and no others).

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/api/generated/alerts apps/frontend/src/api/alerts.ts
git commit -m "feat(frontend): generate AlertRuleItem/FiringItem from alerts.mdl"
```

---

## Task 3: Fix `AlertRuleDetailPage.tsx`/`AlertRuleDetailPage.test.tsx`/`ServiceAlertsTab.test.tsx` type fallout

**Files:**
- Modify: `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx:180`
- Modify: `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx:31-38`
- Modify: `apps/frontend/src/features/services/ServiceAlertsTab.test.tsx:44-57`

- [ ] **Step 1: Fix the `firing.value` check in `AlertRuleDetailPage.tsx`**

Current `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx:180`:

```typescript
                      {firing.value !== null ? firing.value.toFixed(2) : "—"}
```

Replace with:

```typescript
                      {firing.value != null ? firing.value.toFixed(2) : "—"}
```

This is the only change to `AlertRuleDetailPage.tsx` — `firing.resolved_at ? ... : ...` (lines 186-188) is a truthy check and behaves identically for `null` vs `undefined`, no change needed.

- [ ] **Step 2: Fix the `f-1` firing fixture in `AlertRuleDetailPage.test.tsx`**

Current `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx:31-38`:

```typescript
  firings: [
    {
      firing_id: "f-1",
      state: "active",
      value: 95.3,
      occurred_at: "2026-05-18T10:00:05Z",
      resolved_at: null,
    },
```

Replace with (drop `resolved_at: null,`):

```typescript
  firings: [
    {
      firing_id: "f-1",
      state: "active",
      value: 95.3,
      occurred_at: "2026-05-18T10:00:05Z",
    },
```

The `f-2` fixture (lines 39-45, `resolved_at: "2026-05-18T09:30:00Z"`) is unchanged.

- [ ] **Step 3: Fix the `okRule` fixture in `ServiceAlertsTab.test.tsx`**

Current `apps/frontend/src/features/services/ServiceAlertsTab.test.tsx:44-57`:

```typescript
const okRule: alertsApi.AlertRuleItem = {
  rule_id: "rule-2",
  name: "Low Memory",
  metric_name: "memory_free",
  operator: "lt",
  threshold: 10,
  severity: "warning",
  silenced: false,
  state: "ok",
  firing: false,
  last_fired_at: null,
  notification_channels: [],
  auto_trigger_incident: false,
};
```

Replace with (drop `last_fired_at: null,`):

```typescript
const okRule: alertsApi.AlertRuleItem = {
  rule_id: "rule-2",
  name: "Low Memory",
  metric_name: "memory_free",
  operator: "lt",
  threshold: 10,
  severity: "warning",
  silenced: false,
  state: "ok",
  firing: false,
  notification_channels: [],
  auto_trigger_incident: false,
};
```

The `firingRule` fixture (lines 29-42, `last_fired_at: "2026-05-15T10:00:00Z"`) is unchanged.

- [ ] **Step 4: Typecheck and run frontend tests**

```bash
cd apps/frontend && npm run typecheck && npm test -- AlertRuleDetailPage ServiceAlertsTab
```

Expected: typecheck passes with no errors; `AlertRuleDetailPage.test.tsx` and `ServiceAlertsTab.test.tsx` tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx apps/frontend/src/features/services/ServiceAlertsTab.test.tsx
git commit -m "fix(frontend): adapt alert rule pages to optional value/resolved_at/last_fired_at"
```

---

## Task 4: Add lineage doc comments to `services/query-api/src/alerts.rs`

**Files:**
- Modify: `services/query-api/src/alerts.rs:14-15`, `services/query-api/src/alerts.rs:35-36`

- [ ] **Step 1: Add a doc comment above `AlertRuleItem`**

Before (`services/query-api/src/alerts.rs:14-15`):

```rust
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AlertRuleItem {
```

After:

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

- [ ] **Step 2: Add a doc comment above `FiringItem`**

Before (`services/query-api/src/alerts.rs:35-36`):

```rust
#[derive(Serialize)]
pub struct FiringItem {
```

After:

```rust
/// Canonical alert firing entity. Mirrors `alerts.Firing@1` in
/// `models/alerts.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`).
/// `occurred_at`/`resolved_at` stay `chrono::DateTime<Utc>` (Phase 1
/// backlog item 5).
#[derive(Serialize)]
pub struct FiringItem {
```

No other changes — `AlertRuleListResponse`, `AlertRuleDetailResponse`, `AlertRuleRow`, `AlertRuleDetailRow`, `FiringRow`, `CreateRuleRequest`, `SilenceRequest`, `UpdateRunbookRequest`, and all functions/handlers are untouched.

- [ ] **Step 3: Format and build-check**

```bash
cargo fmt --all
cd services/query-api && cargo check
```

Expected: no warnings/errors; `cargo fmt --all` should produce no diff beyond the new comments.

- [ ] **Step 4: Commit**

```bash
git add services/query-api/src/alerts.rs
git commit -m "docs(query-api): cross-reference alerts.mdl AlertRule@1/Firing@1"
```

---

## Task 5: Full verification, lineage proof, mark 3.7 done

**Files:**
- Modify: `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md:98`

- [ ] **Step 1: Lineage proof**

```bash
mkdir -p /c/tmp/alerts-gen/models
cp /c/git/Observable/models/alerts.mdl /c/git/Observable/models/requirements.txt /c/tmp/alerts-gen/models/
cd /c/tmp/alerts-gen && /c/git/modelable/cli/.venv/Scripts/python.exe -m modelable lineage alerts.AlertRule@1 --path models
cd /c/tmp/alerts-gen && /c/git/modelable/cli/.venv/Scripts/python.exe -m modelable lineage alerts.Firing@1 --path models
```

Expected output for `alerts.AlertRule@1`:

```
alerts.AlertRule@1
kind: entity
- ruleId: uuid [key]
- name: string
- metricName: string
- operator: enum(gt, gte, lt, lte, eq)
- threshold: float
- severity: string
- silenced: bool
- state: enum(ok, pending, active, resolved, silenced)
- firing: bool
- lastFiredAt: timestamp
- notificationChannels: array<uuid>
- autoTriggerIncident: bool
```

Expected output for `alerts.Firing@1`:

```
alerts.Firing@1
kind: entity
- firingId: uuid [key]
- state: enum(pending, active, resolved)
- value: float
- occurredAt: timestamp
- resolvedAt: timestamp
```

Save both outputs — they go in the commit/PR description for step 4 below.

- [ ] **Step 2: Frontend verification**

```bash
cd apps/frontend
npm run typecheck && npm run lint && npm test && npm run build
```

Expected: all pass with no new failures.

- [ ] **Step 3: Full local CI**

```bash
bash scripts/local-ci.sh
```

Expected: passes (frontend typecheck/lint/build/test, Rust fmt/clippy/unit tests, Docker image build, smoke test).

- [ ] **Step 4: Mark Phase 3 step 3.7 done**

In `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, change line 98:

Before:

```markdown
- [ ] **3.7 Alerts** — `services/query-api/src/alerts.rs:15-95` (incl. `AlertRuleDetailRow`/`FiringRow`), `apps/frontend/src/api/alerts.ts:5-77`
```

After:

```markdown
- [x] **3.7 Alerts** — Generated `alerts.AlertRule@1`/`alerts.Firing@1` from `models/alerts.mdl` (see `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`). `apps/frontend/src/api/alerts.ts`'s `AlertRuleItem`/`FiringItem` are now re-exports of `apps/frontend/src/api/generated/alerts/alerts.AlertRule.v1.ts`/`alerts.Firing.v1.ts`, including `enum(...)` for `operator` (matches `VALID_OPERATORS`) and both `state` fields (matches the SQL-derived value sets). `services/query-api/src/alerts.rs`'s `AlertRuleItem`/`FiringItem` get lineage doc comments only — timestamp fields can't be generated (Phase 1 backlog item 5, same gap as 3.4-3.6). `value`/`resolved_at`/`last_fired_at: T | null` -> `?: T` required fixing `AlertRuleDetailPage.tsx`'s `firing.value !== null` -> `!= null` and dropping `null` fixtures in `AlertRuleDetailPage.test.tsx`/`ServiceAlertsTab.test.tsx`. `AlertRuleListResponse`, `AlertRuleDetailResponse` remain hand-written.
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md
git commit -m "docs: mark Phase 3 step 3.7 (Alerts) done"
```

---

## Out of scope

- `AlertRuleListResponse`, `AlertRuleDetailResponse`, `AlertRuleRow`, `AlertRuleDetailRow`, `FiringRow`, `CreateRuleRequest`, `SilenceRequest`, `UpdateRunbookRequest` — remain entirely hand-written (see design doc Context/Non-Goals).
- Any change to `services/query-api/src/alerts.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0`.
- Any new Phase 1 backlog items — this migration hits only the already-recorded timestamp gap (item 5).
- `apps/frontend/src/features/alerts/AlertsPage.tsx`/`AlertsPage.test.tsx` (no test file exists for `AlertsPage`) — confirmed zero fallout (`rule.notification_channels ?? []` is nullish-coalescing, safe regardless of optionality; `App.test.tsx` raw `fetch` mocks are untyped object literals).
- `apps/frontend/src/features/services/ServiceAlertsTab.tsx` — confirmed zero fallout (`rule.last_fired_at ? ... : ...` truthy check).
