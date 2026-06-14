# Admin/Members Domain: Modelable Migration Design

**Date:** 2026-06-14
**Status:** Approved
**Scope:** Phase 3 step 3.4 of `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — author `models/admin.mdl` for the `Member` entity, generate and commit a TypeScript artifact, and wire it into `apps/frontend/src/api/admin-members.ts`. Document why `MemberListResponse`, `AddMemberRequest`, `UpdateRoleRequest` remain hand-written, and record a new Phase 1 backlog item for the `timestamp` -> Rust gap.

## Context

`services/query-api/src/admin_members.rs` defines:

- `MemberRecord` (`#[derive(Serialize, sqlx::FromRow)]`): `user_id: Uuid`, `email: String`, `name: Option<String>`, `role: String`, `joined_at: chrono::DateTime<chrono::Utc>`. This single struct serves as **both** the Postgres row (via `sqlx::FromRow`, joining `users`/`user_tenant_roles`) **and** the API response shape returned by `handle_list_members`/`handle_add_member` — there is no separate row/canonical split, unlike tracing/logs/metrics.
- `MemberListResponse { members: Vec<MemberRecord> }` — list wrapper.
- `AddMemberRequest { email: String, role: String }`, `UpdateRoleRequest { role: String }` — request bodies.

`apps/frontend/src/api/admin-members.ts` hand-writes matching types: `MemberRecord` (`user_id`, `email`, `name: string | null`, `role: "tenant_admin" | "member" | "viewer"`, `joined_at: string`), `MemberListResponse`, and `TenantRole = "tenant_admin" | "member" | "viewer"` — a standalone duplicate of `MemberRecord.role`'s type, used independently for form/select state in `MemberManagementPage.tsx`.

### Why TS-only ("Option B"), and a new Phase 1 backlog item

`role: String` in Rust already matches what modelable's `enum(...)` generates in Rust (`String`, per Phase 1 backlog item 3) — so there is no enum-downgrade regression here, unlike 3.2/3.3.

However, `joined_at: chrono::DateTime<chrono::Utc>` is required for `sqlx::FromRow` to map the Postgres `TIMESTAMPTZ` column (`utr.created_at AS joined_at`). modelable's `timestamp` primitive emits as Rust `String` (`_primitive_to_rust` in `cli/src/modelable/emitters/rust.py`), with no `@wire(rust.type: ...)` override available for non-`int` fields (`rust.type` is validated to apply only to `int` fields, `cli/src/modelable/validation/semantic.py:538`). Generating `MemberRecord` in Rust would replace `chrono::DateTime<Utc>` with `String`, breaking the `sqlx::FromRow` mapping — a regression.

So, same as 3.2 (Metrics) and 3.3 (Notifications), this migration is **TS-only**: generate the TypeScript artifact, re-export it into `admin-members.ts`, and add a lineage doc comment to the Rust `MemberRecord` without changing its code.

This is a **new** gap, distinct from item 3 (enum emission). It is recorded as **Phase 1 backlog item 5**: "`timestamp` emits as Rust `String`; no hint exists to target `chrono::DateTime<Utc>` for `sqlx::FromRow`-bound projections."

### `name`: `string | null` -> TS `name?: string`

The hand-written `MemberRecord.name: string | null` reflects that `Option<String>` serializes to `null` (serde's default), not an omitted field. modelable emits optional fields as `name?: string` (i.e. `string | undefined`), confirmed by the existing generated `tracing.Span.v1.ts`'s `parent_span_id?: string` (accepted in 2.5 for the same reason). This is a minor, pre-accepted wire-fidelity gap: `member.name ?? member.email` and `{member.name && (...)}` in `MemberManagementPage.tsx` both behave identically whether the runtime value is `null` or `undefined`. No fallout.

### `AddMemberRequest` / `UpdateRoleRequest` / `MemberListResponse` stay hand-written

Per the Phase 3 "per-domain rule" (handler-local request/wrapper shapes may stay hand-written): `MemberListResponse` is a list wrapper (same precedent as `LogListResponse`/`MetricListResponse`-style types in 3.1/3.2), and `AddMemberRequest`/`UpdateRoleRequest` are request bodies (same precedent as `CreateChannelRequest` in 3.3). All three are unchanged by this migration, in both Rust and TypeScript.

## Goal

- Add `models/admin.mdl` defining `admin.Member@1` (canonical entity), mirroring `MemberRecord` field-for-field.
- Generate and commit a TypeScript artifact (`apps/frontend/src/api/generated/admin/`); re-export it as `MemberRecord` from `apps/frontend/src/api/admin-members.ts`.
- Derive `TenantRole` from the generated `MemberRecord["role"]` (single source of truth for the role union).
- Add a doc comment on `services/query-api/src/admin_members.rs`'s `MemberRecord` cross-referencing `models/admin.mdl` (`Member@1`) for lineage tracking. No Rust code changes beyond this comment.
- Record Phase 1 backlog item 5 (`timestamp` -> Rust `String` gap).
- Mark Phase 3 step 3.4 done in the migration plan.

## Non-Goals

- `MemberListResponse`, `AddMemberRequest`, `UpdateRoleRequest` — all remain hand-written (see Context).
- Any change to `services/query-api/src/admin_members.rs` handler logic, SQL, or Postgres DDL/migrations.
- Bumping the modelable pin — `models/requirements.txt` is already `modelable==0.4.0` and has everything `Member@1` needs (entity + enum + optional + timestamp + `@wire(json.fieldCase: ...)`, all used by prior phases).
- Closing Phase 1 backlog item 5 — only recording it, same as items 1-4 remain open.

## Design

### 1. `models/admin.mdl` (new file)

```
domain admin {
  owner: "platform-team"

  // Canonical tenant-member entity. Mirrors
  // services/query-api/src/admin_members.rs's MemberRecord field-for-field.
  // MemberRecord serves as both the Postgres db row (sqlx::FromRow) and the
  // API response shape; it is NOT modeled in Rust here — see
  // docs/superpowers/specs/2026-06-14-admin-members-modelable-migration-design.md
  // for why (timestamp -> Rust String gap, Phase 1 backlog item 5).
  @wire(json.fieldCase: "snake_case")
  entity Member @ 1 (additive) {
    @key userId: uuid
    email: string
    name?: string
    role: enum(tenant_admin, member, viewer)
    joinedAt: timestamp
  }
}
```

Field-by-field, this mirrors `MemberRecord`: `user_id` (UUID), `email` (string), `name` (optional string -> TS `string | undefined`), `role` (`enum(tenant_admin, member, viewer)` -> TS literal union `"tenant_admin" | "member" | "viewer"`, matching the hand-written `TenantRole` exactly), `joined_at` (`timestamp` -> TS `string`).

No `@wire(json.case: ...)` is needed on `role` — `enum(...)` with no case hint emits values as written, already lowercase-with-underscore, matching both the Rust `role: String` wire values (`tenant_admin`/`member`/`viewer`, written directly by the application) and the hand-written TS literals.

No `projection`/`binding` blocks — `MemberRecord` (the combined db-row/response struct) is not modeled in Rust (see Non-Goals).

### 2. Generated TypeScript artifact

New directory `apps/frontend/src/api/generated/admin/`, containing `admin.Member.v1.ts` (generated via `modelable compile` in an isolated scratch workspace — `admin.mdl` has no `binding`/`projection` blocks, so it compiles standalone, same as `notifications.mdl`/`metrics.mdl`). Same regen-header-comment convention. Expected content:

```typescript
export interface AdminMemberV1 {
  user_id: string;
  email: string;
  name?: string;
  role: 'tenant_admin' | 'member' | 'viewer';
  joined_at: string;
}
export type Member = AdminMemberV1;
```

In `apps/frontend/src/api/admin-members.ts`, replace the hand-written `MemberRecord` interface and `TenantRole` type alias (current lines 9-21) with:

```typescript
import type { Member as MemberRecord } from "./generated/admin/admin.Member.v1";
export type { MemberRecord };

export interface MemberListResponse {
  members: MemberRecord[];
}

export type TenantRole = MemberRecord["role"];
```

`listMembers`, `addMember`, `updateMemberRole`, `removeMember`, `revokeMemberSessions` are unchanged.

### 3. Type-fallout check

`grep` of `MemberRecord`/`TenantRole` usage in `MemberManagementPage.tsx` confirms all field accesses (`user_id`, `email`, `name`, `role`) and `TenantRole` usages (state typing, `ROLES` array, event-handler casts) are structurally compatible with the generated shape. The `name: string | null` -> `name?: string` change (Context section) requires no code changes. Expected fallout: **none**.

### 4. Rust lineage comment

In `services/query-api/src/admin_members.rs`, add a doc comment above `MemberRecord`:

```rust
/// Canonical tenant-member entity. Mirrors `admin.Member@1` in
/// `models/admin.mdl` field-for-field (see
/// `docs/superpowers/specs/2026-06-14-admin-members-modelable-migration-design.md`).
/// Also serves as the Postgres `sqlx::FromRow` projection for the
/// `users`/`user_tenant_roles` join — `joined_at` stays `chrono::DateTime<Utc>`
/// (Phase 1 backlog item 5: modelable's `timestamp` emits as Rust `String`).
#[derive(Serialize, sqlx::FromRow)]
pub struct MemberRecord {
```

No other Rust changes — `MemberListResponse`, `AddMemberRequest`, `UpdateRoleRequest`, and all handlers are untouched.

### 5. Phase 1 backlog item 5

Add to `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`'s "Phase 1 backlog" section:

> 5. **`timestamp` emits as Rust `String`, not a `chrono`/`sqlx`-compatible type.** `_primitive_to_rust` (`cli/src/modelable/emitters/rust.py`) maps `timestamp` -> `String`, and `@wire(rust.type: ...)` is validated to apply only to `int` fields (`cli/src/modelable/validation/semantic.py:538`). Needed for any future Rust generation of types with `sqlx::FromRow`/`clickhouse::Row`-mapped `TIMESTAMPTZ`/`DateTime64` columns, e.g. `admin.Member@1`'s `joinedAt` (`MemberRecord.joined_at: chrono::DateTime<Utc>`, 3.4).

## Verification

- modelable: `cd C:\git\modelable\cli && .venv\Scripts\python.exe -m modelable validate C:\git\Observable\models` passes (`OK 5 files valid.`).
- Lineage proof: `modelable lineage admin.Member@1`, included in the commit/PR description.
- Frontend: `npm run typecheck && npm run lint && npm test && npm run build` from `apps/frontend/`.
- Rust: `cargo fmt --all && cargo check` for `query-api` (doc-comment-only change).
- Full: `bash scripts/local-ci.sh`.
- Mark Phase 3 step 3.4 done and add Phase 1 backlog item 5 in `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`.
