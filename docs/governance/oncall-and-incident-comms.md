# On-Call Sustainability & Out-of-Hours Interruptions Governance

Covers `OBS-37-03` (process) and `OBS-37-04` (process).

## 1. Monthly Out-of-Hours Interruption Review (`OBS-37-03`)

Every month, engineering leadership and on-call leads review out-of-hours page/interruption counts per team, segmented by severity, false-positive rate, and recurring root cause.

## 2. Threshold Breach Action & Ownership (`OBS-37-04`)

Any team exceeding the agreed monthly out-of-hours interruption ceiling records:
- **Assigned owner:** team lead / service owner responsible for remediation.
- **Concrete action:** alert threshold tuning, auto-remediation, or architectural fix with target completion date tracked to closure.
