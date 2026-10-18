# Observability — Service catalog, ownership and coverage

**Version:** 1.0 · **Status:** Draft · **Scope:** Business requirements only

## 2. Service catalog, ownership and coverage

**Business intent:** Establish one accountable view of monitored services and gaps.

| ID | Priority | Requirement | Business acceptance example |
|---|---|---|---|
| OBS-02-01 | P0 | Users shall be able to find a service by its business name, operational name, domain, or owner. | A search using any approved identifier returns the correct service. |
| OBS-02-02 | P0 | Each critical service shall identify its accountable business owner, operational owner, and support route. | Users can view current ownership and escalation responsibility for any critical service. |
| OBS-02-03 | P0 | Each in-scope service shall have a visible monitoring coverage status. | A missing or incomplete coverage case is distinguishable from a healthy service. |
| OBS-02-04 | P0 | Service records shall indicate business criticality and lifecycle status. | Retired services are differentiated from live services in search and reporting. |
| OBS-02-05 | P1 | Owners shall be able to document expected operating hours and planned unavailability. | A planned maintenance period is visible in the affected service context. |
| OBS-02-06 | P1 | Users shall be able to identify services without accountable owners, agreed objectives, or adequate coverage. | An exception report lists each gap and responsible remediation owner. |

