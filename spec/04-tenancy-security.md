# Multi-Tenancy and Security

## 7. Multi-Tenancy

### 7.1 Tenant Models

Support:
1. shared multi-tenant
2. isolated tenant storage
3. single-tenant dedicated deployment

### 7.2 Isolation Levels

- auth isolation
- config isolation
- compute quotas
- storage quotas
- encryption domain separation
- per-tenant rate limits
- optional BYOK

### 7.3 Authorization

Use OIDC/SAML for authn and a fine-grained authorization model for resource access. OpenFGA is a credible choice for relationship-based authorization, designed to evolve beyond simple static roles as access complexity grows.

Recommended model:
- coarse RBAC for tenant admin
- ReBAC for dashboards, projects, environments, incidents, and data scopes

---

## 8. Security Specification

### 8.1 Identity

- OIDC mandatory
- SAML optional
- SCIM provisioning
- workload identity for agents and collectors (see §8.6 for machine-to-machine ingestion tokens)
- short-lived credentials for user-facing and RUM contexts; long-lived ingestion tokens for machine-to-machine ingest (see §8.6)

### 8.2 Data Security

- TLS everywhere
- mTLS for collectors/agents where possible
- encryption at rest
- per-tenant keys optional
- PII classification and masking pipeline
- field-level redaction
- query result access audit

### 8.3 Supply Chain

- signed artifacts
- SBOM required
- provenance attestations
- dependency policy gates
- image scanning
- secret scanning
- IaC scanning

### 8.4 Runtime

- WAF/API protection
- tenant-aware rate limiting
- DDoS controls
- workload isolation
- secret rotation
- break-glass access with audit

### 8.5 Compliance Targets

- SOC 2
- ISO 27001
- GDPR
- regional data boundaries
- audit retention policy

### 8.6 Ingestion Tokens

Ingestion tokens (API keys) are long-lived credentials used exclusively for machine-to-machine telemetry ingest. They are distinct from user credentials and short-lived tokens used in other contexts.

**Cardinality rules:**
- Each ingestion token belongs to exactly one tenant.
- Each ingestion token is scoped to exactly one environment (e.g. `production`, `staging`, `observable`).
- One tenant may have many ingestion tokens (one per environment, plus additional tokens for rotation or tooling).

**Token schema:** `(id, tenant_id, key_hash, name, role, environment, created_at, revoked_at)`  
The `key_hash` is SHA-256 of the plaintext token; the plaintext is never stored.

**Server-side environment resolution:**
The `auth-service` resolves `(tenant_id, role, environment)` from the token on every ingest request. The ingest-gateway stamps all incoming telemetry with the resolved environment before queuing. Clients do not need to configure `deployment.environment` in their OTel SDK — the token alone determines the environment. The client-supplied `deployment.environment` OTel resource attribute is preserved in `resource_attributes` for diagnostics but is not the authoritative `environment` value. See ADR-028 for the full decision record.

**Lifecycle:**
- Tokens can be revoked by setting `revoked_at`. Revoked tokens are rejected immediately at the ingest-gateway.
- Token rotation is accomplished by issuing a new token before revoking the old one.
- Token creation and revocation must be recorded in the audit log.

**Security notes:**
- A client using the wrong token will silently route telemetry to the wrong environment. Operational runbooks must emphasize token-to-environment mapping.
- Ingestion tokens do not grant read or query access; they are write-only credentials at the ingest boundary.

### 8.7 Session Security

- **Cookie Hardening:** All session and PKCE-related cookies must use `HttpOnly`, `SameSite=Lax`, and `Path=/`.
- **Production Safety:** In non-development environments (where `dev_mode` is false), the `Secure` attribute is mandatory for all auth-related cookies.
- **Fail-Closed Authorization:** Any failure to reach the `auth-service` or the underlying session store must result in a rejection of the request (HTTP 503 or 401), ensuring no unauthorized access is granted during dependency outages.
- **Role Enforcement:** All administrative operations (member management, API-key lifecycle, platform configuration) require the `tenant_admin` role. The `member` role is restricted to read-only access for core telemetry and dashboard features.

