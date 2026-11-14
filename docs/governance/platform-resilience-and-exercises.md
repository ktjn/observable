# Platform Resilience, Exercises, and Out-of-Band Response Governance

Covers `OBS-19-06` (process), `OBS-29-04` (process), and `OBS-29-05` (organization).

## 1. Controlled Operational Exercise Outcome Recording (`OBS-19-06`)

Each scheduled platform exercise records a structured comparison of:
- **Expected outcomes:** target detection time, alert delivery channel, and investigation evidence availability.
- **Actual outcomes:** observed detection timestamp, delivered notification receipt, and reconstructed timeline.
- **Variance & remediation:** any gap between expected and actual outcomes logged with an assigned owner and due date.

## 2. Approved Alternative Response Process Drill (`OBS-29-04`)

When primary observability dashboards or alert pipelines are degraded, responders execute the approved out-of-band response runbook:
- Switch to secondary-region `/readyz`, `/metrics`, and failover notification webhooks.
- Record incident timeline and decisions in the offline/secondary responder channel, then reconcile into the primary incident log upon recovery.

## 3. Essential Functions & Failure Scenario Governance Review (`OBS-29-05`)

Quarterly operational resilience reviews record:
- **Essential functions:** critical service health status, critical-alert detection, and responder notification.
- **Failure scenarios:** single-region loss, primary store unavailability, and primary notification route failure.
- **Acceptance evidence:** signed drill reports (`services/admin-service/tests/resilience_drills_integration.rs`) and escalation verification records.
