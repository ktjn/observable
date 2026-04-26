# ADR-024: Deployment Marker Write Path Belongs in the Ingest Gateway

**Date:** 2026-04-26
**Status:** Accepted
**Authors:** Tommy Alander
**Deciders:** Project Stakeholders
**Review date:** 2026-04-26

## Context

Deployment markers (`POST /v1/deployments`, `PATCH /v1/deployments/:id`) are lifecycle events produced by CI/CD pipelines and canary tooling. They need to be stored in PostgreSQL (control-plane metadata, per ADR-002) and made available to the UI for timeline overlays.

Two placement options exist:

- **Ingest gateway:** The service that already receives all external write traffic (OTLP traces, logs, metrics, Prometheus remote_write). Aligns with the "ingest writes, query reads" split that ADR-003 and ADR-009 establish for telemetry signals.
- **Query API:** Already has a `PgPool` connection and is simpler to extend in the short term.

The deciding factor is the future real-time update path. If deployment writes enter the ingest gateway, the gateway can additionally publish lightweight `deployment_started` / `deployment_finished` events to Redpanda (per ADR-009). A future SSE or WebSocket endpoint in the query API can then consume that Redpanda topic and push deployment state changes to connected browser clients without polling. Placing writes in the query API forecloses this path — the query API would have to either poll PostgreSQL itself or receive out-of-band notifications from another service.

## Decision

`POST /v1/deployments` and `PATCH /v1/deployments/:id` are handled by the **ingest gateway**. The ingest gateway writes deployment marker records directly to PostgreSQL for immediate consistency (the `PATCH` lifecycle update must find and modify an existing row).

`GET /v1/deployments` is handled by the **query API**, consistent with all other read endpoints.

**Port:** The deployment write endpoints are served on the **Platform API port (4321)**, not the OTLP ports (4317/4318). The OTLP ports accept only standard OTLP telemetry signals (ADR-001, ADR-023). Port 4321 is the integration surface for Observable-specific authenticated write operations; deployment markers are the first such operation. CI/CD pipelines and tooling target `http://<host>:4321` (env var `OBSERVABLE_URL`).

**Future streaming path (not implemented in this slice):** The ingest gateway should additionally publish a `deployment.events` topic to Redpanda on `POST` and `PATCH`. The query API (or a dedicated notification service) subscribes to that topic and can push real-time deployment state changes to the UI via SSE or WebSocket, eliminating the need for the frontend to poll the `GET /v1/deployments` endpoint.

## Consequences

**Easier:**
- Ingest/query separation is consistent — all writes enter through the ingest gateway, all reads go through the query API.
- The Redpanda event-stream path for real-time UI push is a natural extension: add a `publish_deployment_event` call in the ingest gateway alongside the PostgreSQL write, no architectural change required.
- Auth is already implemented in the ingest gateway (role-checked middleware); viewer-only access for `GET` follows the same pattern already used for query endpoints.

**Harder:**
- The ingest gateway needs a PostgreSQL connection (`PgPool`) added to `AppState`. This is the first PostgreSQL dependency in the ingest gateway; it must be initialized at startup and included in liveness/readiness considerations.
- The two write endpoints bypass Redpanda for now (direct Postgres write). If the team later decides to route deployment events through the stream-processor, the handlers will need to be updated.

**Constrained:**
- Deployment marker writes must go through the ingest gateway. Adding a `/v1/deployments` write endpoint to the query API in the future would violate this ADR.

## Alternatives Considered

### Option A: All three endpoints in the query API
Rejected. Placing writes in the query API couples read and write paths in the wrong service and makes the future Redpanda push path awkward — the query API would need either internal polling or an out-of-band notification channel from the ingest gateway.

### Option B: Writes through Redpanda (ingest gateway → Redpanda → stream-processor → Postgres)
Rejected for this slice. Deployment lifecycle updates (especially `PATCH` to finish a deployment) require reading and modifying an existing row. Routing through Redpanda introduces propagation lag and makes `PATCH` consistency harder without a request-reply pattern. Direct Postgres write is the right choice now; the `deployment.events` Redpanda publication is additive and can be layered on without changing the contract.

## Related

- `spec/18-deployment-markers.md` — full deployment marker schema and API requirements
- `spec/09-api.md §14.2` — "Ingest API" / "Query API" naming already implies this split
- ADR-002: Polyglot Storage — PostgreSQL for control-plane metadata
- ADR-003: ClickHouse Adoption Boundary — PostgreSQL for OLTP/control-plane
- ADR-009: Queue/Stream Backbone — Redpanda as the future event publication path
