# P4-S4: Dashboard ReBAC Design

**Date:** 2026-05-30
**Status:** Approved for implementation
**Slice:** P4-S4 — fine-grained authorization for one protected resource

---

## Goal

Add enforeceable sharing semantics to dashboards using a PostgreSQL-backed relationship tuple store (OpenFGA-style). The ReBAC layer sits on top of the existing RBAC model — it narrows access, never widens it. Existing dashboards and API-key clients are unaffected by default.

---

## Context

The platform already has a coarse RBAC model (`user_tenant_roles`: `tenant_admin`, `member`, `viewer`) implemented in migration `018`. The migration comment explicitly deferred fine-grained authorization to P4-S4. Dashboards are the chosen protected resource because they have a natural sharing need (a user creating a private dashboard, sharing with specific teammates) and are fully CRUD-implemented in `query-api/src/dashboards.rs`.

ADR-008 specifies OpenFGA-influenced ReBAC with PostgreSQL as the tuple store for this slice. Running the actual OpenFGA binary is not required — the tuple format is portable and migration to the OpenFGA service is straightforward if needed later.

---

## Authorization Model

### Principles

1. **Additive**: RBAC remains the outer gate. ReBAC narrows within it.
2. **Identity-scoped**: Tuple checks only apply when `user_id` is present (session/browser auth). API-key callers (`user_id = None`) keep existing tenant-scoped behavior — machines don't have personal identity.
3. **`tenant_admin` bypass**: `tenant_admin` role bypasses all tuple checks and can perform any operation on any dashboard within their tenant.

### Relations

| Relation | Can read | Can write | Can delete |
|---|---|---|---|
| `owner` | ✅ | ✅ | ✅ |
| `editor` | ✅ | ✅ | ❌ |
| `viewer` | ✅ | ❌ | ❌ |

Implicit: a `public` dashboard is readable by any tenant `member` or `viewer` role holder, even with no tuple.

### Access Check Logic

```
can_read(user_id, dashboard):
  if dashboard.visibility == 'public': return true  (member-level access assumed by RBAC gate)
  return has_grant(user_id, dashboard_id, ['owner', 'editor', 'viewer'])

can_write(user_id, tenant_role, dashboard):
  if tenant_role == 'tenant_admin': return true
  return has_grant(user_id, dashboard_id, ['owner', 'editor'])

can_delete(user_id, tenant_role, dashboard):
  if tenant_role == 'tenant_admin': return true
  return has_grant(user_id, dashboard_id, ['owner'])
```

---

## Database

### Migration 030 — add visibility to dashboards

```sql
ALTER TABLE dashboards
  ADD COLUMN visibility TEXT NOT NULL DEFAULT 'public'
    CHECK (visibility IN ('public', 'private'));
```

All existing dashboards default to `'public'` — zero behavior change on migration.

### Migration 031 — dashboard_grants tuple store

```sql
CREATE TABLE IF NOT EXISTS dashboard_grants (
    dashboard_id UUID        NOT NULL REFERENCES dashboards(dashboard_id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id)                ON DELETE CASCADE,
    relation     TEXT        NOT NULL CHECK (relation IN ('owner', 'editor', 'viewer')),
    granted_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (dashboard_id, user_id)
);

CREATE INDEX IF NOT EXISTS dashboard_grants_user_idx
    ON dashboard_grants (user_id, dashboard_id);
```

One row per `(dashboard_id, user_id)` pair — a user holds exactly one relation per dashboard. Updating a grant overwrites the existing relation (upsert).

---

## Service Changes — `query-api`

### `src/dashboards.rs`

**New pure check functions** (single SQL query each, no side effects):

```rust
async fn can_read(db, user_id: Uuid, dashboard_id: Uuid, visibility: &str) -> Result<bool, sqlx::Error>
async fn can_write(db, user_id: Uuid, dashboard_id: Uuid, tenant_role: &str) -> Result<bool, sqlx::Error>
async fn can_delete(db, user_id: Uuid, dashboard_id: Uuid, tenant_role: &str) -> Result<bool, sqlx::Error>
```

**Behavior changes to existing handlers** (apply only when `ctx.user_id` is `Some`):

| Handler | Change |
|---|---|
| `handle_create_dashboard` | After inserting dashboard, insert `(dashboard_id, user_id, 'owner')` grant |
| `handle_list_dashboards` | Filter: include public dashboards + private dashboards where user has any grant |
| `handle_get_dashboard` | After fetch, call `can_read`; return 403 if false |
| `handle_update_dashboard` | Before update, call `can_write`; return 403 if false. Accept optional `visibility` field in `UpdateDashboardRequest`. |
| `handle_delete_dashboard` | Before delete, call `can_delete`; return 403 if false |

**New grant-management handlers:**

| Handler | Route | Auth check |
|---|---|---|
| `handle_list_grants` | `GET /v1/dashboards/{id}/grants` | user has `owner` grant OR `tenant_admin` |
| `handle_add_grant` | `POST /v1/dashboards/{id}/grants` | user has `owner` grant |
| `handle_revoke_grant` | `DELETE /v1/dashboards/{id}/grants/{user_id}` | user has `owner` grant OR `tenant_admin` |

`handle_add_grant` upserts — if a grant already exists for the target user it overwrites the relation. Body: `{ "user_id": "<uuid>", "relation": "viewer"|"editor"|"owner" }`.

`handle_revoke_grant` returns 409 if removing the grant would leave the dashboard with zero `owner` tuples.

**`UpdateDashboardRequest`** gains an optional `visibility` field:
```rust
#[serde(default)]
pub visibility: Option<String>,  // "public" | "private"; None = preserve existing
```

`handle_update_dashboard` validates: if provided, must be `"public"` or `"private"`.

### Route registration in `src/main.rs`

Three new routes added to the dashboard router:

```
GET    /v1/dashboards/:id/grants
POST   /v1/dashboards/:id/grants
DELETE /v1/dashboards/:id/grants/:user_id
```

---

## API Reference

### `GET /v1/dashboards/{id}/grants`

Returns the grant list for a dashboard. Requires `owner` relation or `tenant_admin`.

**Response 200:**
```json
{
  "grants": [
    { "user_id": "...", "relation": "owner", "granted_at": "2026-05-30T..." },
    { "user_id": "...", "relation": "viewer", "granted_at": "2026-05-30T..." }
  ]
}
```

### `POST /v1/dashboards/{id}/grants`

Add or update a grant. Requires `owner` relation. Upserts — overwrites existing relation for the target user.

**Request body:**
```json
{ "user_id": "...", "relation": "viewer" }
```

**Response:** 204 No Content on success. 404 if dashboard not found. 403 if caller is not owner.

### `DELETE /v1/dashboards/{id}/grants/{user_id}`

Remove a grant. Requires `owner` relation or `tenant_admin`.

**Response:** 204 No Content. 404 if grant not found. 409 if removing this grant would leave zero `owner` tuples.

---

## Backward Compatibility

| Scenario | Behavior |
|---|---|
| Existing dashboard (no grants row) | `visibility = 'public'` by default → accessible to all members, unchanged |
| API-key caller (`user_id = None`) | All tuple checks skipped → full existing behavior |
| `tenant_admin` role | Bypasses all tuple checks → can do anything |
| New dashboard created by session user | Owner grant auto-inserted; `visibility = 'public'` by default |
| New dashboard created by API key | No grant inserted; `visibility = 'public'`; accessible to all members |

---

## Testing

### Unit tests (in `src/dashboards.rs`)

- `can_read` returns true for public dashboard regardless of grants
- `can_read` returns false for private dashboard with no grant
- `can_read` returns true for private dashboard with `viewer` grant
- `can_write` returns false for `viewer` grant
- `can_write` returns true for `editor` grant
- `can_write` returns true for `tenant_admin` role regardless of grant
- `can_delete` returns false for `editor` grant
- `can_delete` returns true for `owner` grant
- `can_delete` returns true for `tenant_admin` role regardless of grant

### Integration tests (Testcontainers Postgres, in `tests/dashboard_rebac_integration.rs`)

1. Create dashboard as session user → owner grant exists → creator can edit and delete
2. Share with editor → editor can edit → editor cannot delete
3. Share with viewer → viewer can read → viewer cannot edit
4. Flip to private → non-granted member gets 403 on GET
5. Flip back to public → non-granted member can read again
6. `tenant_admin` can delete any dashboard regardless of grants
7. API-key caller (no user_id) sees all public dashboards, can edit/delete any
8. Revoke last owner grant → 409 response
9. `handle_list_dashboards` for session user shows public dashboards + their private-granted dashboards, not other users' private dashboards

---

## Checkpoint: Is ReBAC Additive to RBAC?

Yes:
- RBAC (`member`/`viewer`/`tenant_admin`) remains the outer authorization gate enforced by the existing `require_tenant` middleware. A request that fails RBAC never reaches tuple checks.
- Tuple checks only execute for session-authenticated users (`user_id` present). API-key callers are unaffected.
- `tenant_admin` role bypasses tuple checks entirely — RBAC role grants full access.
- All existing dashboards default to `visibility = 'public'`, preserving current member read/write access.
- The two models never conflict: RBAC decides who can enter the door; ReBAC decides which dashboards they can touch once inside.
