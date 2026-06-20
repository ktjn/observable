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

Composite alerts evaluate a two-rule pair. The composite rule's `condition` payload references exactly two source rule IDs, stored as `left_rule_id` and `right_rule_id` in the first implementation, and the composite enters Active only when both source rules are Active. If either source rule clears, the composite resolves.

Change-detection alerts compare a current window's metric average against a baseline window
offset N seconds back, firing when the absolute percent change exceeds a configured threshold
(bidirectional — fires on either a spike or a drop). The `condition` payload is
`{metric_name, window_secs, baseline_offset_secs, threshold_percent}`: `window_secs` sizes both the
current window (`[now - window_secs, now]`) and the baseline window
(`[now - baseline_offset_secs - window_secs, now - baseline_offset_secs]`); percent change is
`((current_avg - baseline_avg) / baseline_avg) * 100`. If the baseline window's average is zero,
any non-zero current average fires; if both are zero, the rule does not fire. See
`docs/superpowers/specs/2026-06-20-change-detection-alert-design.md` for the full design rationale.

### 11.2 Incident Model

See `spec/14-domain-model.md §5` for the authoritative Incident entity schema and field definitions.

Operational model:
- **Dedup key:** derived from `rule_id + service_name + environment`; prevents duplicate open incidents when the same condition fires multiple alert evaluations
- **Correlation window:** the platform groups related alerts (same service, overlapping time window) into a single incident rather than creating one incident per alert
- **Impact assessment:** the platform calculates the SLO error budget consumed during the incident window and attaches it to the `slo_impact` field
- **Timeline:** chronological ordered list of responder actions and system events (alert fired, acknowledged, deployment linked, comments); see `IncidentEvent` in `spec/14-domain-model.md §5`
- **Ownership:** incidents are assigned to the on-call responder for the affected service; ownership is resolved via the team membership model in the control plane
- **Runbook link:** stored on the triggering AlertRule (`runbook_url` annotation); surfaced in the incident detail view
- **Status workflow:** see AlertRule and Incident state machines in `spec/14-domain-model.md §5`
- **Post-mortem metadata:** `postmortem_url` on the Incident entity links to an external document; the platform does not host post-mortem documents

### 11.3 Notification Channels

Supported channels:

| Channel | Notes |
|---------|-------|
| Slack | Message to a configured channel or DM; supports `ack` and `resolve` shortcut actions inline |
| Microsoft Teams | Adaptive Card with alert detail; webhook-based |
| PagerDuty | Events API v2; dedup key maps to Incident `dedup_key` |
| Opsgenie | Alert API; dedup key maps to Incident `dedup_key` |
| Email | HTML + plain text; per-user or per-team distribution list |
| Webhook | JSON POST to any HTTPS endpoint; configurable payload template |
| Ticketing system | Jira / Linear / GitHub Issues via adapter plugins |
| ChatOps flows | Slack/Teams bot commands: `ack <incident_id>`, `resolve <incident_id>`, `silence <rule_id> 2h` |

### 11.4 Notification Routing Model

Routing determines which channels receive which alerts. The routing model uses a priority-ordered rule chain:

1. **Rule-level channels:** `notification_channels` on the AlertRule fires when that specific rule changes state. This is the default path for most alerts.
2. **Severity routing:** platform-level routing policy maps `severity = critical` → PagerDuty/Opsgenie; `severity = warning` → Slack; `severity = info` → email. Used when a rule has no explicit channel override.
3. **Team ownership routing:** alerts scoped to a service are routed to channels owned by the team that owns the service, as defined in the project's team-service mapping.
4. **Silence rules:** a silence rule suppresses notifications for matching labels within a defined time window. Silences do not change the alert state machine — they only suppress outbound notifications.
5. **Inhibition rules:** a higher-severity alert for the same service suppresses lower-severity alerts (moves them to `Suppressed` state). This prevents alert storms during major incidents.

**Escalation policy:**
- Each on-call rotation has a configurable escalation policy: if the primary responder does not acknowledge within N minutes, notify the secondary responder or escalation channel.
- Escalation policies are defined per team, not per alert rule, and are resolved at routing time.

---

## 12. SLO and Reliability Management

### 12.1 SLO Entities

See `spec/14-domain-model.md §5` for the authoritative `SLODefinition` schema.

SLO scope types:
- service-level SLO (most common; scoped to `service_name + environment`)
- user journey SLO (spans multiple services; evaluated against synthetic check results or aggregated span sequences)
- API operation SLO (scoped to `service_name + operation_name`)
- dependency SLO (tracks a downstream service from the perspective of the caller)
- tenant-specific SLO (customer-visible reliability commitment)
- platform internal SLO (monitors the observability platform itself; see [spec/17-self-observability.md](17-self-observability.md))

### 12.2 SLIs

| SLI | Measurement source |
|-----|-------------------|
| Request success (availability) | Span `status_code != ERROR` ratio |
| Latency percentiles | Span `duration_ns` distribution; P50, P95, P99 |
| Availability | Span success ratio or synthetic check PASS ratio |
| Freshness | Lag between data ingest timestamp and `timestamp_unix_nano` on the signal |
| Ingest lag | Queue consumer offset lag; time from producer write to storage write |
| Query success | Query facade response `status = success` ratio |
| Alert latency | Time from condition firing to notification delivery |

### 12.3 Burn-Rate Strategy

Implement standard multi-window multi-burn alerts per Google SRE Workbook Chapter 5:

| Window | Burn rate threshold | Budget consumed | Alert urgency |
|--------|---------------------|-----------------|---------------|
| 1h | 14.4× | 2% in 1h | Page immediately |
| 6h | 6× | 5% in 6h | Page |
| 1d | 3× | 10% in 1d | Ticket |
| 3d | 1× | 10% in 3d | Ticket |

Both the fast window and the slow window must be burning simultaneously to fire the alert. This prevents alerts on short spikes that do not threaten the 30-day budget.

---

## 13. Synthetic Monitoring

Synthetic monitoring executes scheduled, externally-originated probes against service endpoints to measure availability and performance from an outside-in perspective. It complements trace and metric data with a ground-truth health signal that is independent of instrumentation.

### 13.1 Check Types

| Type | Description |
|------|-------------|
| HTTP check | Single HTTP/HTTPS request; validates status code, response body, and latency |
| Multi-step HTTP | Sequential HTTP requests (login → add to cart → checkout); validates a user journey |
| gRPC health check | gRPC HealthCheck protocol |
| TCP connectivity | Port reachability and connection latency |
| DNS check | Resolves a hostname and validates the response |
| Browser check (Phase 3+) | Headless browser executes a scripted user journey; captures Core Web Vitals |

### 13.2 Check Definition Entity

| Field | Type | Required | Notes |
|---|---|---|---|
| check_definition_id | UUID | yes | |
| tenant_id | UUID | yes | |
| project_id | UUID | yes | |
| check_name | string | yes | |
| check_type | enum(http, multi_step_http, grpc, tcp, dns, browser) | yes | |
| target_url | string | yes | endpoint under test |
| schedule_cron | string | yes | cron expression defining probe frequency |
| regions | array[string] | yes | probe regions from which to run checks (e.g. `["us-east-1", "eu-west-1"]`) |
| timeout_ms | uint32 | yes | per-check timeout |
| assertions | array[Assertion] | yes | conditions the response must satisfy |
| inject_trace_context | bool | yes | if true, W3C `traceparent` header is injected into probe requests |
| environment | string | no | target environment label |
| service_name | string | no | associated service for SLO and alert correlation |
| enabled | bool | yes | |
| created_by | string | yes | |
| created_at | timestamp | yes | |

**Assertion** (embedded):

| Field | Type | Notes |
|---|---|---|
| assertion_type | enum(status_code, response_body_contains, response_body_json_path, latency_ms, header_value) | |
| operator | enum(eq, neq, lt, lte, gt, gte, contains, matches) | |
| expected_value | string | |

### 13.3 Check Execution Model

- The platform runs probe agents in each configured region as lightweight workers.
- Each probe agent executes the check definition on schedule, records the result as a `SyntheticCheck` telemetry record (see `spec/14-domain-model.md §2`), and writes it to the ingest pipeline.
- Results are stored in the warm tier (30–60d default retention).
- When `inject_trace_context = true`, the resulting `trace_id` is stored on the `SyntheticCheck` record, enabling the `SyntheticCheck → Span` join (see cross-signal join matrix in `spec/14-domain-model.md §3`).

### 13.4 SLO Integration

Synthetic check results can serve as the SLI source for a service SLO:
- `sli_type = availability` can be evaluated against `SyntheticCheck.status = PASS` ratio instead of span data
- This is especially useful for external-facing endpoints where no server-side instrumentation is available
- The `SLODefinition` references `check_definition_id` instead of `service_name` when the SLI source is synthetic
