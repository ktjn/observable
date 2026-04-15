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
- workload identity for agents and collectors
- short-lived credentials only

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
