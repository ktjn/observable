# Incident Classification and Release Review Governance

Covers `OBS-08-08` (organization) and `OBS-11-03` (process).

## 1. Cross-Domain Incident Classification Criteria (`OBS-08-08`)

All product, platform, and infrastructure domains classify incidents using one shared severity matrix based on user impact scope, SLO burn rate, and data integrity risk:

| Severity | Customer / Service Impact | SLO Burn / Degradation | Classification Review |
| :--- | :--- | :--- | :--- |
| `SEV-1` | Broad outage or data-loss risk across core workflows | Critical burn or total unavailability | Immediate commander + post-incident calibration |
| `SEV-2` | Partial degradation or single-tier workflow impairment | Sustained SLO burn without total outage | Weekly cross-domain calibration check |
| `SEV-3` | Localized or workaround-mitigated impact | Minor latency/error budget consumption | Monthly domain sampling |

Weekly operational calibration reviews sample paired incidents from separate domains to confirm identical impact profiles receive identical severity labels under these criteria.

## 2. Release Review Reliability & Customer Gate (`OBS-11-03`)

Every production release record captures explicit pass/fail outcomes for:
1. **Customer criteria:** smoke journeys, checkout/auth health, and absence of user-visible regressions during canary/rollout.
2. **Reliability criteria:** error rate, p95 latency, and SLO burn-rate thresholds remaining within agreed release budgets across equivalent pre/post windows.

A release review is marked `passed` only when both customer and reliability criteria explicitly record `pass`; any failed or unrecorded criterion blocks promotion and triggers rollback/mitigation tracking.
