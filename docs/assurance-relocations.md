# W5: assurance requirements on their providers

Status: **merged** (#1022); implemented on the recommended options E1-E5. W5a (alerting 09-07, 11-05, 17-06; control 11-08, 11-09) merged in #1023. W5b (health 13-07, catalog 15-06, response 16-06; migrations 093-095) delivered and `p2_assurance.rs` removed. W5c exercise results and remaining evidence work are recorded in [section 4.8](#48-platform-exercises-w5c). Plan: [wire-unrouted-capabilities](plans/wire-unrouted-capabilities.md), slice W5.

## 1. Request and baseline

- Request (Tommy, 2026-11-23): "Wire them in." Requirements that are verified only by unit tests of unrouted code get real routes and integration evidence, following the system spec. They are not downgraded.
- Baseline:
  - `services/query-api/src/p2_assurance.rs` holds structs and unit tests only, for OBS-09-07, 11-05, 11-08, 11-09, 13-07, 15-06, 16-06 and 17-06. Nothing is routed or stored. The module sits in query, while most of the targets are provided elsewhere.
  - `services/admin-service/tests/resilience_drills_integration.rs` has five tests that assert literals against themselves: OBS-19-04, 19-05, 19-08, 19-09 and 29-06. The drill tests for 29-01 and 29-02 are real and stay.

## 2. Constraints that shape the design

- The component graph must stay acyclic (COMP-06).
  - Alerting and health require control, so control cannot read alerting or health.
  - Auth is the infrastructure layer, so it cannot read the catalog (COMP-05).
- Evidence must come from the provider's own test paths (COMP-08). A requirement whose behaviour belongs to another provider is retargeted to that provider's capability, in both the area file and the gap file.
- The budgets are full in two places:
  - Alerting provides 6 capabilities and query requires 11. Retargeting to an existing capability adds nothing to either budget.
  - Query cannot take on a new requirement without raising its budget.

## 3. Decisions (approval needed)

| # | Decision | Recommended | Alternative |
| --- | --- | --- | --- |
| E1 | Where cross-domain reviews live (OBS-11-05 delivery versus reliability, OBS-17-06 cost versus outcomes) | **Alerting, under `manage-reliability-objectives`.** Alerting already requires control and already reads control's deployment feed (W4a). It adds `report-observability-cost@^0.1` and composes delivery and cost from control with reliability and outcomes from its own data. | Control keeps them. Reliability and outcome values are then entered by a tenant admin as agreed figures, because control may not read alerting. That is weaker evidence. |
| E2 | Retargets that follow data ownership (COMP-08) | **09-07 → `manage-incidents` (alerting).** **13-07 → `assess-service-health` (health).** **15-06 → `manage-providers` (catalog).** 11-08, 11-09 and 16-06 keep their targets. | Keep the targets and add cross-component reads. 13-07 and 15-06 would then need control → health and auth → catalog, which create a cycle and a layer violation. |
| E3 | The approved figures for the platform exercises (OBS-19-08, 19-09) | **Defaults recorded in the spec, which you can change later:** 200 concurrent users on essential views at p95 ≤ 1.0 s; a planning ceiling of 5,000 spans/s ingest with 500 services and 1,000 rules; detection within one evaluation interval; zero notification loss. The tests read the figures from one file. | You give the figures now. |
| E4 | How OBS-29-06 is shown (detection does not share the monitored region's failure) | **An out-of-band watchdog.** Health probes run from a separate observer deployment with its own notification route. The test stops the monitored stack's containers and asserts that the watchdog detects the loss and notifies through its own route. | Inspection evidence only: a deployment review that the observer runs in another region. This was never shown working. |
| E5 | Delivery | **Three PRs.** W5a: alerting (09-07, 11-05, 17-06) and control (11-08, 11-09). W5b: health (13-07), catalog (15-06) and response (16-06). W5c: platform (19-04, 19-05, 19-08, 19-09, 29-06). | One PR per provider. |

## 4. Behaviour

### 4.1 Similar incidents (OBS-09-07, alerting `manage-incidents`)

- On resolve, the confirmed cause becomes optional on the incident: `{confirmed_cause, cause_category}`.
  - Only the incident owner or a tenant admin sets it.
  - The incident records who confirmed it and when.
- `GET /v1/incidents/{id}/similar?limit=` lists prior incidents in the caller's tenant that share a symptom signature:
  - the same triggering rule, or
  - an overlapping `affected_services` with the same severity.
- Each result shows the matched signals and the confirmed cause if there is one. Results with a confirmed cause rank first.
- Negative: incidents from another tenant, and incidents with no shared signal, are never listed.

### 4.2 Delivery and reliability review (OBS-11-05, alerting `manage-reliability-objectives`)

- `GET /v1/objectives/delivery-review?from=&to=&service_id=` returns two separate sections with no combined score.
  - `delivery`, from control deployments: deployment count, change failure rate (failed or rolled back), and median time to restore after a failed change. Each figure carries `source: control`.
  - `reliability`, from approved objectives: attainment and error budget. Each figure carries `source: alerting`.
- Only measures a tenant admin approved for the review are shown: `PUT /v1/objectives/review-measures`.
- Negative: no field adds or ratios a delivery measure with a reliability measure.

### 4.3 Partial release comparison (OBS-11-08, control `track-changes`)

- `GET /v1/deployments/{id}/rollout-comparison?window=` compares two populations for the deployment's service and environment over the window, using ClickHouse spans:
  - exposed: `service_version` equals the deployment's version
  - unexposed: any other version
- For each population it reports request count, error rate and p95 latency.
- A population below a minimum sample (default 100 requests) is reported as `insufficient_data`, never as zero.

### 4.4 Change freezes (OBS-11-09, control `track-changes`)

- `control.change_freezes`: tenant, name, scope (environment and optionally services), `starts_at`, `ends_at`, `approved_by`. A tenant admin creates it.
- `POST /v1/deployments/{id}/freeze-exception {reason}` records an exception. The person recording it must be a tenant admin other than the deployer.
- `GET /v1/change-freezes/{id}/changes` lists every in-scope deployment in the window, each marked `exception` (with approver and reason) or `unapproved`.

### 4.5 Scheduled status reports (OBS-13-07, health `assess-service-health`)

- `POST /v1/health/report-schedules {scope: {journey_ids|service_ids}, period: weekly|monthly}` creates a schedule as `pending_approval`.
  - A second tenant admin approves it.
  - Only approved schedules run.
- A worker writes report snapshots for each period. A snapshot holds status and trend for exactly the scoped services, with each entry's own scope and freshness.
- `GET /v1/health/report-schedules/{id}/reports` reads the snapshots with the caller's catalog visibility.
- Delivery by email is out of scope.

### 4.6 Third-party disruption exposure (OBS-15-06, catalog `manage-providers`)

- Provider dependencies gain `regulated: bool`, set by a tenant admin.
- `POST /v1/catalog/providers/{id}/incidents {title, started_at, material}` records a provider incident.
- `GET /v1/catalog/providers/{id}/incidents/{incident_id}/exposure` lists each affected regulated function. Each entry gives the function, the service or journey it belongs to, and the dependency record.
- Negative: a non-material incident yields no exposure entries, and hidden services are omitted.

### 4.7 Shared operational view (OBS-16-06, response `exchange-operational-context`)

- `POST /v1/response/shared-views {incident_id, participants: [principal ids]}` creates the view.
  - The creator must be able to read the incident (`manage-incidents`, with their own credential).
  - Response adds `lookup-principals@^0.1` to validate the participants.
- Participants post coordination notes, and every participant reads the same timeline.
- Every read checks two things: that the caller is a participant, and the caller's current access to the incident.
- Negative: a non-participant, or a participant who has lost access to the incident, gets 404.

### 4.8 Platform exercises (W5c)

Results from 2026-11-28. The local Docker Desktop daemon had 16 CPUs and approximately 30.9 GiB available. The Compose frontend image used the locally built production bundle in the pinned runtime stage. The browser and load runs used `481133f4`. The corrected default growth run used the pre-commit local harness based on `f3859bfb`. The successful watchdog run and the clean 50 ms comparison used the unpublished local branch `fix/w5c-platform-exercise-fixtures` at `4156abab`, based on `e1d9262a`.

- The local CI frontend browser lane runs the [W5c Playwright cases](../apps/frontend/e2e/w5c-critical-workflows.spec.ts). To run just these cases from the repository root, use `npm run test:a11y --workspace=apps/frontend -- e2e/w5c-critical-workflows.spec.ts`.
- With Compose running, `bash scripts/platform-exercises.sh` runs load, growth and watchdog using the [approved exercise figures](../config/exercises/platform-exercises.json). `bash scripts/local-ci.sh --containers --exercises` includes these exercises after the container smoke tests. [Exercise runner](../scripts/platform-exercises.sh) · [Local CI](../scripts/local-ci.sh)

| Requirement | Exercise result |
| --- | --- |
| OBS-19-04 | **Passed, five Chromium cases.** The production frontend with mocked API responses passed the WCAG 2.2 AA axe checks on the incident list, incident detail, service health and alert feed. A responder acknowledged and resolved an incident using only the keyboard. |
| OBS-19-05 | **Passed, one Chromium case.** The Tokyo and New York browser contexts showed the same incident events in the same UTC order, with the expected local times and zone labels. |
| OBS-19-08 | **Passed, live Compose.** Over 120 seconds, 200 users completed 23,580 requests across six essential views with zero errors. The highest per-view p95 was 48.2 ms against the 1 s limit. |
| OBS-19-09 | **Failed, default 500 ms storage-writer flush.** The gateway accepted 1,515,500 spans at 5,001 spans/s. ClickHouse contained 474,000 after the 10 s drain, leaving 1,041,500 accepted spans unaccounted for at observation. The run saw 500 services and 1,000 active rules. None of the ten canary metrics became queryable or produced a recorded firing or notification during the exercise. The six essential views had zero errors and a maximum p95 of 67.5 ms. |
| OBS-19-09 | **Passed, isolated Windows Docker Desktop stack with `STORAGE_WRITER_FLUSH_INTERVAL_MS=50`.** ClickHouse held all 1,515,500 accepted spans at the first observation. All ten canaries had a recorded firing and matching webhook receipt. No quiet rule fired. Maximum detection time was 1.538 s and maximum dispatch time was 9.989 s. All six essential views had zero errors and a maximum p95 of 167.7 ms. |
| OBS-29-06 | **Passed, live Compose.** The external watchdog notified loss for all five targets within 16.142 s of the stop (25 s budget), then notified recovery for all five. All 15 captured containers returned to running state. The preceding drill notified all five losses in 16.130 s and only four recoveries. |

Both growth profiles targeted 5,000 spans/s across 500 services, with 1,000 active rules and 50 concurrent essential-view users for 300 s. The first growth run used a different fixture tenant from the ingest credential. The table reports the corrected default run and the clean 50 ms comparison. The deployed default remains 500 ms, and its acceptance remains open.

The four OBS-19 specification records still reference the original unit tests. The OBS-29-06 record references the process drill. Replace the records and retire the corresponding obsolete tests as the real exercises pass and the code gate permits delivery. Any OBS-19-09 passing evidence must identify the tested profile.

## 5. Verification

Every API row is an integration test through the routes, run as the provider's own login. Each requirement has its acceptance case and its negative case from section 4. The unit tests in `p2_assurance.rs` and the five tautological tests are deleted once their replacements pass.

## 6. Risks

- E1 makes alerting the place where cross-domain reviews live. The alternative weakens the evidence instead.
- The defaults in E3 are assumptions until you replace them.
- E4 still runs on one machine. It shows that the watchdog's failure domain is independent, not that a real second region exists.
