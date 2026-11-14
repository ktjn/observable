# Error-Budget Exhaustion Policy (`OBS-06-08`)

**Owner:** SRE Lead  
**Scope:** Production and business-critical services managed under approved SLOs (`manage-reliability-objectives`).

## 1. Purpose

When a service exhausts its error budget (`remaining_error_budget_ratio <= 0.0` for its evaluation window), service owners and engineering leadership must agree and record the operational consequence rather than continuing unreviewed feature releases.

## 2. Agreed Consequences on Exhaustion

Upon error-budget exhaustion, the accountable service owner selects and records one of the following actions:

1. **`freeze_non_critical_releases`**: Halt non-reliability feature deployments to the affected service until error budget recovers or an explicit exception is signed off.
2. **`prioritize_reliability_work`**: Dedicate sprint capacity to root-cause remediation, rollback hardening, or dependency resilience until attainment returns above target.

## 3. Decision Record & Accountable Approver

Every exhausted budget record must capture:
- `policy_applied`: `freeze_non_critical_releases` or `prioritize_reliability_work`
- `decision_taken`: concise summary of the agreed operational action and effective window
- `accountable_approver`: named service owner or SRE lead approving the decision (and any exception)
- `recorded_at`: UTC timestamp of sign-off

Exceptions to a release freeze require dual sign-off by the accountable service owner and the Head of Operations.
