# ADR-023: Standard OTLP Port Conformance

## Status

Accepted

## Context

The OpenTelemetry Protocol (OTLP) specification defines standard ports for ingestion:
- **4317**: OTLP/gRPC
- **4318**: OTLP/HTTP

Prior to this ADR, the Observable platform used a non-standard port layout:
- `ingest-gateway` served OTLP/HTTP on port **4317**.
- `auth-service` occupied port **4318**.

This layout prevented standard OTel components (collectors, SDKs, edge tools) from reaching Observable without custom endpoint configuration.

The ingest-gateway also hosts non-OTLP platform write operations (e.g. deployment
markers). These must not share the OTLP ports — port 4318 must remain strictly OTLP
to preserve the OTLP contract (ADR-001) and allow independent network-level routing.

## Decision

The Observable platform port assignments are:

1. **Ingest Gateway** serves three listeners:
   - **OTLP/gRPC** on port **4317** (using `tonic`). Port 4317 does not serve OTLP/HTTP.
   - **OTLP/HTTP JSON** on port **4318** (using `axum`). Port 4318 accepts `application/json` only; no non-OTLP routes are registered on this port.
   - **Platform API** on port **4321** (using `axum`). Non-OTLP, Observable-specific authenticated write operations (e.g. deployment markers). Configured via `INGEST_GATEWAY_PLATFORM_PORT` (default `4321`).
2. **Auth Service** uses an internal-only port: **4319**.

## Consequences

- **Standard Port Alignment**: OTel components use port **4317** for gRPC and port **4318** for OTLP/HTTP JSON (for example, `OTLP_ENDPOINT=http://observable:4318` with an HTTP/JSON exporter).
- **Compatibility Note**: OTLP/HTTP protobuf (`application/x-protobuf`) is not supported on port **4318**. HTTP clients must send JSON.
- **Platform API**: Non-OTLP Observable platform writes (deployment markers, and future additions) target port **4321**. CI/CD pipelines and tooling must use `OBSERVABLE_URL=http://<host>:4321`.
- **Service Reconfiguration**:
    - `ingest-gateway` runs three concurrent server listeners (gRPC, HTTP/OTLP, Platform API).
    - `auth-service` internal validation endpoint is at `http://auth-service:4319/internal/validate`.
- **Infrastructure Impact**:
    - `docker-compose.yml` updated to reflect new port mappings.
    - Helm charts (`charts/observable`) updated for multi-port support in `ingest-gateway`.
    - Local dev environment and documentation (`spec/12-deployment.md`) updated.
- **Migration Path**: Existing deployments must update their `AUTH_SERVICE_URL` and `INGEST_GATEWAY_PORT` environment variables. OTLP senders should point to 4317 for gRPC and 4318 for HTTP JSON. Deployment marker tooling should point to port 4321.

## Verification

- `cargo check -p ingest-gateway` verifies multi-server listener logic and OTLP proto integration.
- `cargo test -p ingest-gateway` verifies rate limiting on new HTTP routes.
- `docker compose up` smoke checks verify port availability and service-to-service communication.
