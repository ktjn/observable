# Changelog

All notable changes to the `observable-store-clickhouse` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Telemetry read-visibility watermarks (`PUSH-016`, `publish-telemetry-readonly-changes`): `WriteBuffer` coalesces flushed span, log, and metric-point batches per `(tenant_id, signal)` into a single `TelemetryWatermarkEvent` (`telemetry.watermarks.v1`) and publishes it best-effort after ClickHouse commit when `TELEMETRY_WATERMARKS_TOPIC` is configured.

### Fixed

- Configure the non-replicated insert deduplication window on all five telemetry
  tables through schema migration 005 instead of an invalid query setting.
- Run the real-ClickHouse retry-deduplication regression by default, including
  logs, metrics, span events, and repeated migration application.

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Consumption of `telemetry.normalized.v1` with at-least-once delivery and bounded retry/backpressure.
- ClickHouse writes for spans, logs, metric series/points, and span events.
- Ownership of `migrations/clickhouse/*.sql` and the declared schema version
  (`migrations/clickhouse/SCHEMA_VERSION`).
- `/readyz` ClickHouse probe and retention handling.
