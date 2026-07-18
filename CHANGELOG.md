# Changelog

All notable changes to the Observable platform will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-18

Initial public release of Observable.

### Added

- **Ingestion**
  - OTLP traces, logs, and metrics support over gRPC (port 4317) and HTTP/Protobuf/JSON (port 4318).
  - Prometheus Remote Write (`remote_write`) compatibility.
  - Deployment markers (`POST /v1/deployments`) and change events (`POST /v1/events/changes`).
  - Tenant-aware ingestion with API keys.
  - Machine-readable OpenAPI 3.1 contracts for all public APIs.

- **Storage & Processing**
  - Polyglot storage: ClickHouse for telemetry, PostgreSQL for control-plane metadata.
  - Stream-processor (Rust) for in-flight normalization and RED metrics generation.
  - Multi-tenant isolation enforced at the storage and query layers.
  - Versioned schema migrations for both ClickHouse and PostgreSQL.

- **Query & Visualization**
  - Unified Query API (Rust) with Arrow/DataFusion execution layer.
  - Trace search and deep-link navigation between traces and related logs.
  - Metric exploration and dashboarding with reusable panels.
  - Log explorer with histogram and severity-based filtering.
  - Service catalog and topology maps derived from OTel resource attributes.

- **Alerting & Reliability**
  - Threshold alerts, SLO burn-rate alerts, and Deadman (no-data) alerts.
  - Composite alert evaluation.
  - Incident timeline and topology-aware impact analysis.
  - Outbound webhook notifications.

- **Security & Administration**
  - OIDC browser authentication and session management.
  - Fine-grained authorization via OpenFGA.
  - `admin-service` for isolated member management and API-key lifecycle.
  - Tenant-scoped audit logs for queries and configuration changes.

- **Deployment & Operations**
  - Supported Docker Compose `evaluation` profile.
  - Production-ready Helm v3 chart with library chart pattern.
  - Platform self-observability: every service emits OTLP traces, metrics, and logs.
  - Local verification gates: `local-ci.sh`, `smoke-test.sh`, `perf-smoke.sh`, and `kind-test.sh`.
  - Automated verification of migration idempotency and upgrade procedures.

- **Supply Chain**
  - Tag-bound artifact builds (OCI images and Helm charts).
  - Multi-architecture container images (`linux/amd64`, `linux/arm64`).
  - SPDX SBOMs and SLSA provenance attestations for container images.
- Automated release artifact signing and attestation workflows.

### Fixed

- Resolved CI build caching issues for Rust and C/C++ dependencies.
- Synchronized Helm chart versions with platform release version.
- Hardened OIDC callback failure paths and session security.
- Fixed migration idempotency and upgrade reliability.

[0.1.0]: https://github.com/ktjn/observable/releases/tag/v0.1.0
