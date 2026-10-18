# Changelog

All notable changes to the `observable-process` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Consumption of `telemetry.raw.v1` and normalization/enrichment of spans, logs, and metrics.
- RED-metric generation and service/environment resource enrichment.
- Publication of the durable `telemetry.normalized.v1` boundary consumed by storage.
