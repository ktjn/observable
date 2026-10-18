# Observability — Dependencies and business impact

**Version:** 1.0 · **Status:** Draft · **Scope:** Business requirements only

## 10. Dependencies and business impact

**Business intent:** Understand the blast radius and prioritize what matters.

| ID | Priority | Requirement | Business acceptance example |
|---|---|---|---|
| OBS-10-01 | P0 | Users shall be able to identify which business services depend on an affected service or external provider. | A dependency failure shows its immediate business service exposure. |
| OBS-10-02 | P0 | Incident severity assessments shall consider impacted critical business processes and customer journeys. | Two incidents with similar technical symptoms can receive different impact priorities. |
| OBS-10-03 | P1 | Authorized users shall be able to explore direct and indirect dependencies between services. | A second-order dependency is discoverable during impact assessment. |
| OBS-10-04 | P1 | Users shall be able to see known downstream and upstream effects during an incident. | A representative shared-service outage identifies potentially affected consumers. |
| OBS-10-05 | P1 | Dependencies on material third-party services shall be visible to relevant business owners. | A third-party outage is linked to in-scope affected business services. |
| OBS-10-06 | P2 | Business owners shall be able to estimate the scale of impact using approved business measures. | An impact view provides the agreed measure and marks estimates as estimates. |

