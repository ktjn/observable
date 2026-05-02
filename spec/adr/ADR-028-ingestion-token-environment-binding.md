# ADR-028: Ingestion Token — Per-Token Environment Binding

**Date:** 2026-05-02  
**Status:** Accepted  
**Authors:** Observable Engineering  
**Deciders:** Project Stakeholders  
**Review date:** 2026-05-02  

## Context

Observable is a multi-tenant observability platform. Each tenant may have multiple environments (e.g. `production`, `staging`, `observable`). Telemetry signals must be tagged with the correct environment so that queries and dashboards filter reliably across environment boundaries.

Two approaches were considered to stamp the `environment` dimension on ingested signals:

1. **Trust the client:** read `deployment.environment` from the OTel resource attributes in the OTLP payload.
2. **Server-side resolution:** resolve environment from the ingestion token and stamp all signals at the ingest boundary, ignoring the client-supplied attribute.

The client-trust approach is simple but has two material problems:

- **Spoofability:** any client holding a valid API key can claim any environment value, including `production`, without restriction. This creates a route for accidental or malicious data contamination across environment boundaries.
- **Zero-config requirement for internal services:** Observable's own Rust services send telemetry via the OTel SDK. Requiring each service to be individually configured with a `deployment.environment` SDK resource attribute creates operational overhead and a class of misconfiguration bugs (service emitting to wrong environment). A token-scoped environment eliminates that footgun: the token determines the environment, and services need no per-service configuration.

Migration `014_add_environment_to_api_keys.sql` added an `environment` column to the `api_keys` table and seeded initial tokens (`observable-api-key-0000` → `observable`, `dev-api-key-0000` → `testbench`). The `auth-service.lookup_api_key` RPC was extended to return `(tenant_id, role, environment)`. The ingest-gateway stamps all incoming telemetry with the resolved environment before queuing.

## Decision

1. **Each ingestion token (`api_key`) has exactly one `environment`.** The environment is declared at token creation time and cannot change without issuing a new token.
2. **Each ingestion token belongs to exactly one tenant.** There is no cross-tenant token.
3. **The `auth-service` resolves `(tenant_id, role, environment)` from every ingest request token.** This triple is returned to the ingest-gateway on each authenticated request.
4. **The ingest-gateway stamps `environment` server-side on all telemetry** using the token-resolved value before the payload is placed on the durable queue.
5. **The client-supplied OTel resource attribute `deployment.environment` is not trusted as the authoritative environment source.** It is preserved in `resource_attributes` for diagnostic purposes but does not populate the top-level `environment` column on any signal schema.
6. **Clients do not need to configure `deployment.environment` in their OTel SDK.** The correct environment is entirely determined by which token the client presents at ingest.

## Consequences

**Easier:**
- Environment spoofing is structurally prevented; a token can only route to its declared environment.
- Observable's own internal services require zero per-service environment configuration.
- New environments can be onboarded by issuing a new token; no SDK reconfiguration needed.
- Environment-scoped access control (future: restrict a token to read-only, or to a specific environment subset) is straightforward to layer on top.

**Harder:**
- A client using the wrong token will silently route telemetry to the wrong environment. Operational runbooks must emphasize token-to-environment mapping.
- The `deployment.environment` OTel resource attribute is no longer the authoritative source for the `environment` column, which deviates from the vanilla OTel semantic conventions contract defined in ADR-001. Existing documentation and third-party tooling that assumes `deployment.environment` controls environment placement will be misleading.
- Token lifecycle management (rotation, revocation) becomes a first-class operational concern. A revoked token stops all telemetry from the associated client immediately.

**Constrained:**
- No telemetry may enter a tenant's data store without a token-resolved `(tenant_id, environment)` pair. Unauthenticated ingest or ingest with an unrecognized/revoked token is rejected at the ingest-gateway.
- The `environment` column in all signal schemas is a server-stamped value, not a pass-through of the OTel attribute. Query and dashboard code must treat it as a first-class partition key, not as a derived attribute.

## Alternatives Considered

### Option A: Trust `deployment.environment` OTel resource attribute
The ingest-gateway would read `deployment.environment` from the payload resource attributes and use it as the `environment` column value. Clients would be responsible for setting the correct attribute.

Rejected because: (a) spoofable — any client can claim any environment; (b) requires per-service SDK configuration; (c) creates silent data contamination risk across environments.

### Option B: Require both token environment and OTel attribute to match; reject on mismatch
The ingest-gateway would resolve environment from the token and validate it matches the `deployment.environment` resource attribute. Mismatch would produce a 4xx rejection.

Rejected because: (a) imposes OTel attribute configuration burden on clients that are already constrained by the token; (b) breaks zero-config internal services; (c) complicates migration of existing OTel-instrumented services that already set `deployment.environment`.

### Option C: Environment declared per-collector, not per-token
Environments would be configured in the collector/forwarder (e.g. OTel Collector `resource` processor), not tied to the API key.

Rejected because: it still relies on client-side configuration and does not prevent spoofing from clients that bypass the collector.

## Related

- `spec/14-domain-model.md` §1 (Entity Glossary — `ApiKey` entity), §2 (signal schemas — `environment` field), §7 (Common Dimensions Reference)
- `spec/02-architecture.md` §4.1 (Ingestion pipeline — environment resolution step)
- `spec/04-tenancy-security.md` §8.6 (Ingestion Tokens)
- `spec/adr/ADR-001-otel-external-contract.md` (OTLP as ingest contract; `deployment.environment` attribute preserved but not authoritative for environment column)
- `spec/adr/ADR-007-multi-tenant-isolation.md` (tenant_id enforced at ingest)
- `spec/adr/ADR-008-authorization-model.md` (API key store)
- `migrations/postgres/014_add_environment_to_api_keys.sql`
