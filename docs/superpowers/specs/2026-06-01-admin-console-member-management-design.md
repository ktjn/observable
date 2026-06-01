# Admin Console — Member Management Design

**Date:** 2026-06-01  
**Status:** Approved  
**Scope:** RBAC mutation controls for the admin console — tenant admins can list, invite, re-role, remove, and session-revoke members.

---

## 1. Context

The admin console currently shows a read-only view of the current user's tenant memberships in `TenantConfigurationPage`. No endpoint or UI exists to mutate `user_tenant_roles`. This slice adds a dedicated **Members** tab where tenant admins can manage who has access to the selected tenant.

Quota editing is explicitly out of scope for this slice.

---

## 2. Backend

### 2.1 New file

`services/query-api/src/admin_members.rs`

### 2.2 Endpoints

All five endpoints require `tenant_admin` role. Non-admins receive 403. The `tenant_id` is always taken from the auth context — never from the request body.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/v1/admin/members` | List all users in the current tenant. Joins `users` + `user_tenant_roles` on `tenant_id`. Returns `id`, `email`, `name`, `role`, `created_at`. |
| `POST` | `/v1/admin/members` | Add user by email. Body: `{ email, role }`. Looks up `users.email`; if not found returns 404. Inserts into `user_tenant_roles`. |
| `PUT` | `/v1/admin/members/:user_id/role` | Update role. Body: `{ role }`. Rejects if `user_id` matches the caller (self-demotion guard → 403). |
| `DELETE` | `/v1/admin/members/:user_id` | Remove user from tenant. Rejects if removing the last `tenant_admin` (→ 403). Also revokes all active sessions for that user in this tenant. |
| `POST` | `/v1/admin/members/:user_id/revoke-sessions` | Sets `revoked_at = now()` on all active (non-revoked, non-expired) sessions for the given user scoped to this tenant. |

### 2.3 Guards

- **Self-demotion:** `PUT .../role` returns 403 if `user_id == auth_ctx.user_id`.
- **Last-admin lockout:** `DELETE` returns 403 if after removal no `tenant_admin` would remain.
- **Cascade on remove:** `DELETE` revokes sessions in the same transaction (avoids a race between remove and an in-flight session check).

### 2.4 No migration needed

`user_tenant_roles` and `user_sessions` already exist with the correct schema.

---

## 3. Frontend

### 3.1 New route

`/admin/members` → `MemberManagementPage`

Added to `AdminSurfaceNav` between "Tenant configuration" and "Fleet management".

### 3.2 New files

| File | Purpose |
|------|---------|
| `apps/frontend/src/api/admin-members.ts` | Five typed API functions matching the backend endpoints |
| `apps/frontend/src/features/admin/MemberManagementPage.tsx` | Page component |
| `apps/frontend/src/pages/AdminMembersPage.tsx` | Re-export shim (`export { MemberManagementPage as default }`) |

### 3.3 Existing files to update

| File | Change |
|------|--------|
| `apps/frontend/src/features/admin/AdminSurfaceNav.tsx` | Add `{ to: "/admin/members", label: "Members" }` |
| Router (`routeTree.gen.ts` / route definitions) | Register `/admin/members` |
| MSW handlers | Add handlers for all five endpoints |

### 3.4 Page layout

```
Administration / Members
Manage who has access to this tenant.

[Overview] [Tenant config] [Members] [Fleet] [Identity]

┌── Add member ──────────────────────────────────────────┐
│  Email ___________________  Role [Member ▾]   [Add]    │
│  (inline error if email not found)                     │
└────────────────────────────────────────────────────────┘

┌── Members (N) ─────────────────────────────────────────┐
│  Name / Email          Role           Actions           │
│  Alice alice@…         [Admin]        [Role ▾]  [×]    │
│  Bob   bob@…           [Member]       [Role ▾]  [×]    │
│  You   you@…           [Viewer]       (read-only)       │
│         ↳ Revoke sessions (link, per row)               │
└────────────────────────────────────────────────────────┘
```

### 3.5 Behaviors

- **Add member form** — visible only to `tenant_admin`. Email input + role selector (`member` default) + submit. Shows inline field error on 404 ("No account found for that email").
- **Role select** — inline `<select>` per row for other users. Fires `PUT` on change with optimistic update via React Query `useMutation`. Self row shows read-only badge.
- **Remove (×)** — calls `window.confirm` then fires `DELETE`. Sessions revoked server-side automatically.
- **Revoke sessions** — small secondary link per row. Fires `POST .../revoke-sessions`. Shows success toast on completion.
- **Non-admins** — page is accessible but all mutation controls (`<select>`, ×, revoke, add form) are hidden. Table remains readable.
- **Empty state** — if the members list is empty or fails to load, use the existing `EmptyState` component.

### 3.6 Role access guard

```ts
const isAdmin = currentMembership?.role === "tenant_admin";
```

All mutation JSX gated on `isAdmin`.

---

## 4. Error Handling

| Scenario | Backend | Frontend |
|----------|---------|----------|
| Email not found | 404 | Inline field error below email input |
| Self-demotion | 403 | Toast: "You cannot change your own role" |
| Last admin removal | 403 | Toast: "Cannot remove the last admin from a tenant" |
| Network / unknown error | 5xx | React Query error state via existing `ErrorState` component |

---

## 5. Testing

### Backend (HTTP integration tests — `services/query-api/tests/http_api_integration.rs`)

- `GET /v1/admin/members` returns members for tenant admin
- `GET /v1/admin/members` returns 403 for non-admin
- `POST /v1/admin/members` adds existing user; returns 404 for unknown email
- `PUT /v1/admin/members/:id/role` updates role; returns 403 for self
- `DELETE /v1/admin/members/:id` removes user; returns 403 when last admin
- `DELETE /v1/admin/members/:id` revokes sessions as a side effect
- `POST /v1/admin/members/:id/revoke-sessions` marks sessions revoked

### Frontend (MSW + RTL)

- Add form: success path, email-not-found error
- Role select: change fires mutation, optimistic update visible
- Remove: confirm dialog → member disappears from list
- Non-admin: all controls absent from DOM

---

## 6. Out of Scope

- Quota editing (separate slice)
- Full invitation flow with email delivery (deferred — add-by-email only works for existing accounts)
- Fleet Management live inventory (separate slice)
- Bulk operations
