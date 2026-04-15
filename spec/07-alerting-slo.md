# Alerting, Incidents, and SLOs

## 11. Alerting and Incident Management

### 11.1 Alert Types

- static threshold
- anomaly
- change detection
- deadman
- composite
- topology impact
- SLO burn rate
- deployment regression

### 11.2 Incident Model

- dedup key
- correlation window
- impact assessment
- timeline
- ownership
- runbook link
- status workflow
- postmortem metadata

### 11.3 Notification Channels

- Slack
- Teams
- PagerDuty/Opsgenie equivalent
- email
- webhook
- ticketing system
- chatops ack/close flows

---

## 12. SLO and Reliability Management

### 12.1 SLO Entities

- service
- user journey
- API operation
- dependency
- tenant-specific SLO
- platform internal SLO

### 12.2 SLIs

- request success
- latency percentiles
- availability
- freshness
- ingest lag
- query success
- alert latency

### 12.3 Burn-Rate Strategy

Implement standard multi-window multi-burn alerts.
