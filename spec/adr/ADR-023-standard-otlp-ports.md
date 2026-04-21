# ADR-023: Standard OTLP Port Conformance

## Status

Proposed

## Context

The OpenTelemetry Protocol (OTLP) specification defines standard ports for ingestion:
- **4317**: OTLP/gRPC
- **4318**: OTLP/HTTP

Prior to this ADR, the Observable platform used a non-standard port layout:
- `ingest-gateway` served OTLP/HTTP on port **4317**.
- `auth-service` occupied port **4318**.

This layout prevented standard OTel components (collectors, SDKs, edge tools) from reaching Observable without custom endpoint configuration.

## Decision

We will align the Observable platform with the OTLP standard port assignments:

1.  **Ingest Gateway** will now serve:
    - **OTLP/gRPC** on port **4317** (using `tonic`).
    - **OTLP/HTTP** on port **4318** (using `axum`).
2.  **Auth Service** will move to an internal-only port: **4319**.

## Consequences

- **Standard Compliance**: Standard OTel components can now send data to Observable using default settings (e.g., `OTLP_ENDPOINT=http://observable:4318`).
- **Service Reconfiguration**:
    - `ingest-gateway` now runs two concurrent server listeners (HTTP and gRPC).
    - `auth-service` internal validation endpoint is now at `http://auth-service:4319/internal/validate`.
- **Infrastructure Impact**:
    - `docker-compose.yml` updated to reflect new port mappings.
    - Helm charts (`charts/observable`) updated for multi-port support in `ingest-gateway`.
    - Local dev environment and documentation (`spec/12-deployment.md`) updated.
- **Migration Path**: Existing deployments must update their `AUTH_SERVICE_URL` and `INGEST_GATEWAY_PORT` environment variables. Standard OTLP senders should point to 4317 (gRPC) or 4318 (HTTP).

## Verification

- `cargo check -p ingest-gateway` verifies multi-server listener logic and OTLP proto integration.
- `cargo test -p ingest-gateway` verifies rate limiting on new HTTP routes.
- `docker compose up` smoke checks verify port availability and service-to-service communication.
