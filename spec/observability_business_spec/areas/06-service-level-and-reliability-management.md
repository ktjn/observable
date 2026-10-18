# Observability — Service-level and reliability management

**Version:** 1.0 · **Status:** Draft · **Scope:** Business requirements only

## 6. Service-level and reliability management

**Business intent:** Manage reliability against customer-relevant commitments.

| ID | Priority | Requirement | Business acceptance example |
|---|---|---|---|
| OBS-06-01 | P0 | Service owners shall be able to define service-level indicators (SLIs) for customer-relevant outcomes. | An owner can identify the measured outcome and calculation scope for each critical SLI. |
| OBS-06-02 | P0 | Service owners shall be able to approve service-level objectives (SLOs) and evaluation periods. | A critical service has an approved target, time period, and accountable owner. |
| OBS-06-03 | P0 | Users shall be able to distinguish objectives, internal commitments, and contractual service-level agreements. | A review cannot confuse an internal SLO with a contractual SLA. |
| OBS-06-04 | P0 | The platform shall show attainment, breaches, and remaining error budget for agreed SLOs. | A known period of failures changes attainment and remaining budget as expected. |
| OBS-06-05 | P1 | Users shall be warned when a service is at risk of breaching its objective during the evaluation period. | A simulated accelerated consumption of error budget generates an at-risk state. |
| OBS-06-06 | P1 | Management shall be able to review reliability results by service, business area, and reporting period. | A monthly review correctly distinguishes passing and failing objectives. |
| OBS-06-07 | P1 | Objective definitions and target changes shall remain reviewable over time. | A historical report uses the target in force for the selected period. |

