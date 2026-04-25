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
    - **OTLP/gRPC** on port **4317** (using `tonic`). Port 4317 does not serve OTLP/HTTP.
    - **OTLP/HTTP JSON** on port **4318** (using `axum`). Port 4318 accepts `application/json` and does not accept `application/x-protobuf`.
2.  **Auth Service** will move to an internal-only port: **4319**.

## Consequences

- **Standard Port Alignment**: OTel components use port **4317** for gRPC and port **4318** for OTLP/HTTP JSON (for example, `OTLP_ENDPOINT=http://observable:4318` with an HTTP/JSON exporter).
- **Compatibility Note**: OTLP/HTTP protobuf (`application/x-protobuf`) is not supported on port **4318**. HTTP clients must send JSON.
- **Service Reconfiguration**:
    - `ingest-gateway` now runs two concurrent server listeners (HTTP and gRPC).
    - `auth-service` internal validation endpoint is now at `http://auth-service:4319/internal/validate`.
- **Infrastructure Impact**:
    - `docker-compose.yml` updated to reflect new port mappings.
    - Helm charts (`charts/observable`) updated for multi-port support in `ingest-gateway`.
    - Local dev environment and documentation (`spec/12-deployment.md`) updated.
- **Migration Path**: Existing deployments must update their `AUTH_SERVICE_URL` and `INGEST_GATEWAY_PORT` environment variables. OTLP senders should point to 4317 for gRPC and 4318 for HTTP JSON.

## Verification

- `cargo check -p ingest-gateway` verifies multi-server listener logic and OTLP proto integration.
- `cargo test -p ingest-gateway` verifies rate limiting on new HTTP routes.
- `docker compose up` smoke checks verify port availability and service-to-service communication.
