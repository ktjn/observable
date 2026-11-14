# Alert Ownership and Escalation Policy (`OBS-07-08`)

Every critical alert rule and service-level burn-rate alert in `observable-alerting` MUST declare an accountable owner (`owner_kind` + `owner_id`) and a defined escalation path.

## Escalation Path & Interval
1. **Primary Responder / Response Group:** Critical alerts notify the designated primary responder or response group immediately upon firing (`OBS-07-01`).
2. **Primary Unavailable Failover:** When the primary responder is unavailable, `route_critical_alert` fails over directly to the designated fallback accountable responder.
3. **Unacknowledged Escalation SLA:** Any critical alert that remains unacknowledged beyond the agreed **15-minute (900-second)** acknowledgement interval (`evaluate_escalation_due`) escalates to the next approved responder (`POST /v1/incidents/{incident_id}/escalate`).
4. **Audit Retention:** Every acknowledgement and escalation is recorded in `incident_events` (`acknowledged`, `escalated`) and retained across incident resolution/closure (`OBS-07-04`).
