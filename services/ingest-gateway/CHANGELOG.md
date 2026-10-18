# Changelog

All notable changes to the `observable-ingest` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- OTLP trace, log, and metric ingestion over gRPC (4317) and HTTP (4318).
- Prometheus Remote Write (`remote_write`) ingestion.
- Ingestion-token authentication with tenant/environment resolution and stamping.
- Publication to the `telemetry.raw.v1` Redpanda topic.
- Deployment-marker registry fed by `deployment.markers.v1` (no PostgreSQL dependency).
