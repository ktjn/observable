# Changelog

All notable changes to the `observable-store-clickhouse` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
