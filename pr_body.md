### Summary
- Completed P0 release blocker: full auth and tenant-isolation regression coverage.
- Hardened session security with cookie attribute enforcement and fail-closed dependency handling.
- Enforced `tenant_admin` role across all administrative handlers in `admin-service`.

### Changes
- **Security Hardening**:
  - Added `Secure` flag to session and PKCE cookies in non-dev environments.
  - Ensured `HttpOnly`, `SameSite=Lax`, and consistent `Path=/` for all auth cookies.
  - Documented session security and role enforcement in `spec/04-tenancy-security.md`.
- **Role-Based Access Control**:
  - Moved `require_admin` guard to shared middleware and applied it to 10+ administrative endpoints (tokens, alerts, config, usage, members).
  - Verified that users with `member` role are strictly forbidden from administrative mutations.
- **Verification & Tests**:
  - Added `services/admin-service/tests/admin_roles_integration.rs` covering RBAC.
  - Added cross-tenant role enforcement tests in `auth_tenant_isolation_integration.rs`.
  - Added hardened cookie attribute assertions in `services/auth-service/tests/oidc_http.rs`.
  - Verified all existing `admin-service` and `auth-service` tests pass with the new security gates.

### Verification
- Ran `cargo test -p auth-service` (45 tests passed).
- Ran `cargo test -p admin-service` (48 tests passed).
- Verified cookie attributes manually via test assertions.
- Confirmed that `auth-service` outage results in 503 Service Unavailable (fail-closed).
