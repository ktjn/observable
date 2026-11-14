# Critical Function Monitoring Ownership & Control Register

This register demonstrates that monitoring responsibility exists for all critical business functions (`OBS-15-01`).

## Critical Function Ownership Register

Every critical business function MUST declare:

- **Function ID** (`function_id`): unique identifier of the critical business function
- **Criticality** (`criticality`): `tier-0` | `tier-1`
- **Monitoring Owner** (`monitoring_owner`): accountable team or role responsible for SLOs, alerts, and telemetry coverage
- **Monitoring Scope** (`monitoring_scope`): covered services, user journeys, and signals
- **Control Status** (`control_status`): `compliant` | `control_failure`
- **Last Reviewed** (`last_reviewed`): ISO-8601 date of last compliance verification

| Function ID | Criticality | Monitoring Owner | Monitoring Scope | Control Status | Last Reviewed |
| --- | --- | --- | --- | --- | --- |
| `checkout-payments` | `tier-0` | `Payments Reliability Team` | Traces, logs, error-rate SLOs for `checkout` and `billing-core` | `compliant` | `2026-11-01` |
| `identity-and-auth` | `tier-0` | `Platform Security Team` | Auth latency, token validation, and deny-rate alerts for `auth-service` | `compliant` | `2026-11-01` |
| `telemetry-ingestion` | `tier-0` | `Observability Platform Team` | Ingest gateway throughput, Kafka lag, and ClickHouse durability | `compliant` | `2026-11-01` |
| `legacy-settlement-batch` | `tier-1` | `unassigned` | Nightly settlement job | `control_failure` (flagged — missing monitoring owner) | `2026-11-01` |

## Unowned Critical Function Control Failure

Any critical business function whose `monitoring_owner` is empty, `none`, or `unassigned` MUST be reported as `control_failure` in compliance audits and blocks sign-off until an accountable owner is assigned and verified.
