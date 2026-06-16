# Notifications Domain: Modelable Migration Design

**Date:** 2026-06-14
**Status:** Approved
**Scope:** Phase 3 step 3.3 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/notifications.mdl` for the `NotificationChannel` entity, generate and commit a TypeScript artifact, and wire it into `apps/frontend/src/api/notifications.ts`. Document why `NotificationChannelItem` (db row), `CreateChannelRequest`, and `NotificationChannelType` remain hand-written in Rust.

## Context

`services/query-api/src/notifications.rs` defines four types relevant to Phase 3 scoping:

- `NotificationChannelType` — a real Rust enum (`#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] #[serde(rename_all = "lowercase")] pub enum NotificationChannelType { Webhook }`), with a hand-written `From<String>` fallback conversion.
- `NotificationChannelItem` — the Postgres `db` row (`sqlx::FromRow`): `channel_id: Uuid`, `name: String`, `channel_type: String` (`#[sqlx(rename = "type")]`), `config: serde_json::Value`.
- `NotificationChannelResponse` — the canonical API response: `channel_id: Uuid`, `name: String`, `channel_type: NotificationChannelType`, `config: serde_json::Value`. Built from `NotificationChannelItem` via `From<NotificationChannelItem> for NotificationChannelResponse` (line 43), which converts `channel_type: String` -> `NotificationChannelType` via `.into()`.
- `CreateChannelRequest` — request body: `name: String`, `channel_type: NotificationChannelType`, `config: serde_json::Value`.

`apps/frontend/src/api/notifications.ts` hand-writes matching types: `NotificationChannelType = "webhook"` (literal union), `NotificationChannelConfig = {url: string; [key: string]: unknown}`, `NotificationChannelItem` (`channel_id`, `name`, `channel_type`, `config: NotificationChannelConfig`), and `CreateChannelRequest` (same shape as the Rust request type).

### Why TS-only ("Option B"), and why `CreateChannelRequest` stays hand-written

`NotificationChannelResponse` is the canonical entity-equivalent shape (`channel_id`, `name`, `channel_type: NotificationChannelType`, `config: serde_json::Value`) and maps cleanly to a modelable entity. However, modelable's Rust emitter (`cli/src/modelable/emitters/rust.py`, `_shape_base_annotation`) currently emits all `enum(...)` IDL types as `String` (Phase 1 backlog item 3, recorded during 3.2). Generating `NotificationChannelResponse` in Rust would replace the real `NotificationChannelType` enum (with its `rename_all = "lowercase"` serde derive and `From<String>` fallback) with a bare `String` field — a regression in type safety. So, same as 3.2 (Metrics), this migration is **TS-only**: generate the TypeScript artifact, re-export it into `notifications.ts`, and add a lineage doc comment to the Rust canonical type without changing its code.

`CreateChannelRequest` is a request-body type, not a canonical domain/wire entity returned by the API — per the Phase 3 "per-domain rule" (handler-local request/validation shapes may stay hand-written), it is out of scope in both Rust and TypeScript. It continues to reference the hand-written `NotificationChannelType`/`NotificationChannelConfig` types in TS and `NotificationChannelType` in Rust, both of which are unchanged by this migration.

`NotificationChannelItem` (Postgres `db` row, `sqlx::FromRow`) also stays hand-written — same rationale as `MetricPointRow` in 3.2: modeling a `db` projection would require either generating `sqlx::FromRow` derives (a separate emitter capability) or accepting a `channel_type: String` shape, and there is no TS consumer of the row type to motivate generating it. It is unchanged by this migration.

### The `config` field: `json` -> TS `unknown`

modelable's `json` primitive type maps to TS `unknown` (`_type_to_ts` in `cli/src/modelable/emitters/typescript.py`). The hand-written `NotificationChannelConfig = {url: string; [key: string]: unknown}` is more specific. There is no inline-object-type or `@manual`-shape precedent in this codebase's `.mdl` files for narrowing `json` fields (tracing's `attributes`/`resourceAttributes` use `map<string, json>` -> `Record<string, unknown>`, same limitation).

Generating `config: json` (-> `unknown`) is accepted as a deliberate, minor type-tightening: the single usage that depends on `NotificationChannelConfig`'s `url: string` field (`apps/frontend/src/features/alerts/NotificationChannelsList.tsx:125`, `channel.config.url`) is fixed with a cast to the existing hand-written `NotificationChannelConfig` type: `(channel.config as NotificationChannelConfig).url`. `NotificationChannelConfig` itself remains hand-written and exported from `notifications.ts` for this purpose and for `CreateChannelRequest`.

## Goal

- Add `models/notifications.mdl` defining `notifications.NotificationChannel@1` (canonical entity), mirroring `NotificationChannelResponse` field-for-field.
- Generate and commit a TypeScript artifact (`apps/frontend/src/api/generated/notifications/`); re-export it as `NotificationChannelItem` from `apps/frontend/src/api/notifications.ts`.
- Fix the resulting `config: unknown` fallout at `NotificationChannelsList.tsx:125` with a cast to `NotificationChannelConfig`.
- Add a doc comment on `services/query-api/src/notifications.rs`'s `NotificationChannelResponse` cross-referencing `models/notifications.mdl` (`NotificationChannel@1`) for lineage tracking. No Rust code changes beyond this comment.
- Mark Phase 3 step 3.3 done in the migration plan.

## Non-Goals

- `NotificationChannelItem` (Postgres `db` row), `CreateChannelRequest`, `NotificationChannelType` (Rust enum), `NotificationChannelConfig` (TS type) — all remain hand-written (see Context).
- Any change to `services/query-api/src/notifications.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything `NotificationChannel@1` needs (entity + enum + json + `@wire(json.fieldCase: ...)`, all used by prior phases).
- Recording new Phase 1 backlog items — this migration hits only the already-recorded enum-emission gap (item 3) and the already-recorded `json` -> `unknown` narrowing (handled the same way as 3.2).

## Design

### 1. `models/notifications.mdl` (new file)

```
domain notifications {
  owner: "platform-team"

  // Canonical notification-channel entity. Mirrors
  // services/query-api/src/notifications.rs's NotificationChannelResponse
  // field-for-field. NotificationChannelItem (the Postgres db row) and
  // CreateChannelRequest are intentionally NOT modeled here — see
  // docs/superpowers/specs/2026-06-14-notifications-modelable-migration-design.md
  // for why (Rust enum(...) emission and json->unknown narrowing).
  @wire(json.fieldCase: "snake_case")
  entity NotificationChannel @ 1 (additive) {
    @key channelId: uuid
    name: string
    channelType: enum(webhook)
    config: json
  }
}
```

Field-by-field, this mirrors `NotificationChannelResponse`: `channel_id` (UUID), `name` (string), `channel_type` (`enum(webhook)` -> TS literal union `"webhook"`, matching the hand-written `NotificationChannelType` exactly), `config` (`json` -> TS `unknown`).

No `@wire(json.case: ...)` is needed on `channelType` — `enum(webhook)` with no case hint emits the value as written (`"webhook"`), which is already lowercase and matches both the Rust `#[serde(rename_all = "lowercase")]` wire value and the hand-written TS literal.

No `projection`/`binding` blocks — `NotificationChannelItem` (db row) is not modeled (see Non-Goals).

### 2. Generated TypeScript artifact

New directory `apps/frontend/src/api/generated/notifications/`, containing `notifications.NotificationChannel.v1.ts` (generated via `modelable compile` in an isolated scratch workspace — `notifications.mdl` has no `binding`/`projection` blocks, so it compiles standalone, same as `metrics.mdl`; see Phase 1 backlog item 4 for why the full `models/` directory can't be compiled from scratch). Same regen-header-comment convention as `metrics.MetricPoint.v1.ts`. Expected content:

```typescript
export interface NotificationsNotificationChannelV1 {
  channel_id: string;
  name: string;
  channel_type: "webhook";
  config: unknown;
}
export type NotificationChannel = NotificationsNotificationChannelV1;
```

In `apps/frontend/src/api/notifications.ts`, replace the hand-written `NotificationChannelItem` interface (current lines 12-17) with a re-export aliased to the existing name:

```typescript
export type { NotificationChannel as NotificationChannelItem } from "./generated/notifications/notifications.NotificationChannel.v1";
```

`NotificationChannelType`, `NotificationChannelConfig`, and `CreateChannelRequest` remain hand-written and exported as before. `listNotificationChannels`, `createNotificationChannel`, `deleteNotificationChannel` are unchanged.

### 3. Type-fallout fix

`NotificationChannelItem.config` changes from `NotificationChannelConfig` (`{url: string; [key: string]: unknown}`) to `unknown`. The only field access through `config` is `apps/frontend/src/features/alerts/NotificationChannelsList.tsx:125`:

Before:
```tsx
<div className="truncate text-xs text-[var(--muted)]">{channel.config.url}</div>
```

After:
```tsx
<div className="truncate text-xs text-[var(--muted)]">{(channel.config as NotificationChannelConfig).url}</div>
```

This requires importing `NotificationChannelConfig` (type-only) into `NotificationChannelsList.tsx`.

`grep` confirms `NotificationChannelItem` is otherwise only referenced via `channel_id`/`name`/`channel_type` (in `NotificationChannelsList.tsx` and `AlertsPage.tsx`'s `ChannelsCell`), all unaffected by this change. `App.test.tsx`'s `/v1/notifications/channels` fixtures (if any) are `JSON.stringify`'d mock responses, not type-annotated as `NotificationChannelItem` — no change needed (same as the 3.1/3.2 precedent).

Expected fallout: **one call site**, fixed above.

### 4. Rust lineage comment

In `services/query-api/src/notifications.rs`, add a doc comment above `NotificationChannelResponse`:

```rust
/// Canonical notification-channel entity. Mirrors `notifications.NotificationChannel@1`
/// in `models/notifications.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-notifications-modelable-migration-design.md`).
#[derive(Serialize, Debug, Clone)]
pub struct NotificationChannelResponse {
```

No other Rust changes — `NotificationChannelItem`, `NotificationChannelType`, `CreateChannelRequest`, and the `From` impl are untouched.

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes (`OK 4 files valid.`).
- Lineage proof: `modelable lineage notifications.NotificationChannel@1`, included in the commit/PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build` from `apps/frontend/`.
- Rust: `cargo fmt --all && cargo check` for `query-api` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.3 done in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
