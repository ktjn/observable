# Changelog

All notable changes to the `observable-query` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Trace, log, metric, and service query APIs over ClickHouse.
- Cross-signal discovery and reliability reports (merging control/alerting `/internal/*` enrichment).
- Natural-language query (NLQ) pipeline with an OpenAI-compatible LLM backend.
- Query audit log and the declared ClickHouse storage-schema compatibility range
  (`services/query-api/clickhouse-schema-support.yaml`).
