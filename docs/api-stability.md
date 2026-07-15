# API Stability Expectations

Observable is pre-1.0 software. This document describes what callers and
deployers can expect from public interfaces during the 0.x release series.

## Versioning

Observable follows [Semantic Versioning 2.0.0](https://semver.org/). During the
0.x series, minor version bumps (0.1 to 0.2) may include breaking changes.
Patch releases (0.1.0 to 0.1.1) will not break existing behavior.

## Ingestion endpoints

| Endpoint | Stability |
|----------|-----------|
| `POST /v1/traces` (OTLP/HTTP JSON) | Stable within 0.1.x |
| `POST /v1/logs` (OTLP/HTTP JSON) | Stable within 0.1.x |
| `POST /v1/metrics` (OTLP/HTTP JSON) | Stable within 0.1.x |
| OTLP/gRPC (port 4317) | Stable within 0.1.x |
| `POST /api/v1/write` (Prometheus Remote Write) | Stable within 0.1.x |

These endpoints follow the OpenTelemetry and Prometheus specifications. Breaking
changes to their behavior will only occur if required by an upstream spec change
or a security fix.

## Query and administration APIs

The query API (`/api/query/*`) and admin API (`/api/admin/*`) are internal to
Observable's frontend and are **not considered stable**. They may change between
any releases. External tooling should not depend on these endpoints without
accepting breakage risk.

## Storage schemas

ClickHouse table schemas (column names, types, sort keys) and PostgreSQL schemas
may change between minor versions. Migrations are forward-only and idempotent.
Downgrades are not supported — see [backup-restore.md](backup-restore.md).

## Configuration

Environment variables and Helm values documented in [deployment.md](deployment.md)
are stable within a minor version series. New required variables will be introduced
only in minor version bumps and will cause fail-closed startup errors with
actionable messages.

## What "stable" means

- The endpoint path, method, and content type will not change.
- Required request fields will not be added.
- Successful response codes will not change.
- Error response codes may be added for new validation.
- Response body fields may be added but not removed or renamed.

## What may change without notice

- Internal error message text (do not parse error strings).
- Response headers beyond those required by the relevant specification.
- Rate-limit quotas and thresholds.
- Logging and metrics emitted by Observable itself.
- The set of unsupported metric types that are silently dropped.
