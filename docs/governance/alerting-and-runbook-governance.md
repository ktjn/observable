# Alerting & Operational Guidance Governance (`OBS-07-07`, `OBS-33-02`, `OBS-33-04`)

**Owner:** SRE Lead  
**Scope:** Alert evaluation (`evaluate-alerts`) and operational guidance (`manage-operational-guidance`).

## 1. Alert-Quality Review Process (`OBS-07-07`, `holder: process`)

Every operational team conducts a periodic alert-quality review covering a defined reporting period (monthly or post-major incident):
- **Missed alerts:** incidents where customer impact occurred without a timely page.
- **False positives:** pages that fired without actionable service degradation.
- **Recurring noise:** rules firing repeatedly and auto-resolving without intervention.

Each review report records summary counts per category plus concrete alert rule examples and the remediation action (threshold adjustment, dedup tuning, or retirement).

## 2. Runbook Ownership & Staleness Review (`OBS-33-02`, `holder: organization`)

Every operational runbook linked from an alert rule or service catalog entry must carry:
- `owner`: accountable team or service owner
- `review_date`: next scheduled review date (at most 180 days from last revision)
- `status`: `current`, `needs_review`, or `retired`

Quarterly organization reviews list all runbooks past their `review_date` alongside their accountable owners so stale guidance is visibly flagged and updated.

## 3. Post-Incident Runbook Feedback Loop (`OBS-33-04`, `holder: process`)

During post-incident review:
1. Responders record whether linked runbook guidance was **used** and whether it was **effective** during mitigation.
2. Any missing, inaccurate, or outdated step triggers a documented runbook update.
3. The post-incident review record captures the **updated runbook step** and the **accountable approver** before closure.
