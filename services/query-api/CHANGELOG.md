# Changelog

All notable changes to the `observable-query` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Platform confidence (phase 2 E6, OBS-29-03): `GET /v1/platform/confidence` composes the health worker self-status (`assess`), the alert evaluator self-status (`detect`) and the caller's tenant ingest watermark (`ingest`, against `INGEST_WATERMARK_BOUND_SECONDS`, default 300). Confidence is `full` only when every function is current; a stale, delayed or unreadable function is `degraded` and named in `affected`; a function that has not completed a pass, or a tenant with no telemetry in 24 h, is `unknown`. Self-statuses are read with `INTERNAL_SERVICE_TOKEN`.
- Service reviews (`GET /v1/review/services[/{id}]`) now carry `scope`, `period` (`as_of`, the rule's `freshness_seconds`), `confidence`, and per service `freshness` (status, last evidence, age, bound), open data-quality `issues` and `fit_for_decision` with `unfit_reasons` (OBS-26-01). An issue list that cannot be read is `evaluation_unavailable` with `open: null`, never zero.
- `GET /v1/overview/services?environment_class=` lists business-critical services with state, evidence age, open issues and fitness, plus the platform confidence they depend on (OBS-03-01, OBS-03-03).

### Fixed

- Infrastructure entities without spans reported `health_state: healthy` because a missing error rate defaulted to 0. They now report `unknown` (OBS-24-02).

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
