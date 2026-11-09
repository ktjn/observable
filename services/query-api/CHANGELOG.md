# Changelog

All notable changes to the `observable-query` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Removed

- The derived `health_state` (healthy, watch, breach from error rate) on `/v1/services/summary`
  and `/v1/services/{name}/summary` (phase 1 C4.3). It called a service with no requests healthy
  and mixed environments. Summaries now carry `slo_breaching`, a firing legacy SLO burn-rate
  alert, as a fact; health and agreed expectations come from `/v1/review/services`.
  Infrastructure entity health is unchanged.

### Added

- Service review (phase 1 C4, OBS-01-03, OBS-01-04, OBS-02-04):
  `GET /v1/review/services/{id}?environment_class=` and
  `GET /v1/review/services?environment_class=&context=business_critical|all` combine the catalog
  record, `assess-service-health` and approved objective compliance with the caller's own
  credential. Expectation status is `compliant`, `at_risk`, `breached` or `unknown` with a reason;
  missing, stale, withheld or unavailable evidence and unapproved objectives are never compliant.
  Retired and planned services are not live and carry no health or expectation. Needs
  `CATALOG_URL` and `HEALTH_URL`; without them the review answers 503.

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Trace, log, metric, and service query APIs over ClickHouse.
- Cross-signal discovery and reliability reports (merging control/alerting `/internal/*` enrichment).
- Natural-language query (NLQ) pipeline with an OpenAI-compatible LLM backend.
- Query audit log and the declared ClickHouse storage-schema compatibility range
  (`services/query-api/clickhouse-schema-support.yaml`).
