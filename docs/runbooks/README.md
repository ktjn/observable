# Operational Runbooks

This directory contains operator-facing runbooks for production failure classes.

Current coverage:

- [Deployment regression runbook](deployment-regression.md) — canary or rollout failures where the safest recovery is to roll back to the last known-good release and verify health before retrying promotion.
- [Ingest pipeline and queue backpressure runbook](ingest-queue-failure.md) — diagnosing OTLP 429/503 errors, API key validation bottlenecks, and Redpanda queue saturation.
- [Storage writer and ClickHouse recovery runbook](storage-recovery.md) — ClickHouse write stalls, disk space saturation, partition recovery, and consumer lag drainage on `telemetry.normalized.v1`.
- [Identity and authorization runbook](auth-identity-failure.md) — Zitadel OIDC token exchange failures, session secret rotation, and OpenFGA authorization timeouts.
- [Dependency outage and bounded degradation runbook](dependency-outage.md) — matrix of bounded-degradation behaviors during partial infrastructure loss and safe whole-cluster startup sequence.
