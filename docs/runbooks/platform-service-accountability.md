# Observable Platform Service Accountability & Out-of-Band Escalation

This document establishes accountability for the Observable platform as an internal business service (`OBS-19-03`).

## Owner, Support, and Operating Commitments

- **Accountable Service Owner:** `observability-platform-owner` (Platform Reliability Engineering Lead)
- **Primary Support Route:** `#obs-platform-support` (business hours) and `obs-platform-oncall` Pager rotation (`24x7` for Severity 1 / Severity 2 platform incidents)
- **Operating Commitments:**
  - **Ingest Availability:** `99.9%` monthly availability for OTLP and Prometheus Remote Write ingestion
  - **Query & Dashboard Availability:** `99.9%` monthly availability for `query-api` and UI
  - **Alerting & Notification Latency:** `p99 <= 60s` from breach detection to responder notification
  - **Initial Response Targets:** `<= 15 min` for critical platform incidents, `<= 1 hour` for major degradation

## Out-of-Band Platform Escalation

When the Observable platform itself is degraded or unavailable, incident detection, ownership lookup, and escalation MUST NOT depend on Observable UI, Observable dashboards, or Observable-hosted routing:

1. **Named Platform Incident Owner:** `observability-platform-owner` (Primary: Platform Reliability Lead; Backup: Infrastructure Engineering Director), maintained in the out-of-band corporate directory and printed in the static SRE handbook.
2. **Independent Health Signals:** External synthetic probes and `kube-prometheus-stack` out-of-cluster Alertmanager monitor `/health` and `/readyz` on all Observable services (`ingest-gateway`, `stream-processor`, `storage-writer`, `query-api`, `auth-service`, `admin-service`, `alert-evaluator`).
3. **Independent Escalation Route:** Alerts route directly from external Alertmanager to the out-of-band paging provider (SMS/voice/mobile push) and emergency incident bridge, bypassing Observable's own notification pipeline.
4. **Static Recovery Procedures:** Service-level runbooks in `docs/runbooks/` are stored in Git and mirrored to offline/static runbook storage so responders can execute recovery without accessing the Observable UI.
