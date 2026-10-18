# Observability — Health and availability

**Version:** 1.0 · **Status:** Draft · **Scope:** Business requirements only

## 3. Health and availability

**Business intent:** Provide an accurate operational picture of service condition.

| ID | Priority | Requirement | Business acceptance example |
|---|---|---|---|
| OBS-03-01 | P0 | Users shall be able to see the current health of every in-scope critical service. | A service overview shows normal, degraded, unavailable, or unknown condition. |
| OBS-03-02 | P0 | Users shall be able to distinguish unavailable, degraded, planned maintenance, and lack-of-evidence states. | A test of each condition yields a distinct, correctly labeled status. |
| OBS-03-03 | P0 | The platform shall show when a service last had sufficient evidence to assess its health. | Stale or absent evidence cannot be mistaken for a healthy service. |
| OBS-03-04 | P0 | Users shall be able to inspect recent and historical service interruptions and degradations. | A known outage can be located with its onset, duration, and resolution status. |
| OBS-03-05 | P1 | Services shall be assessable across relevant operating regions, channels, and customer segments where applicable. | A regional incident is visible without incorrectly labeling all segments as affected. |
| OBS-03-06 | P1 | The platform shall identify recurring health problems affecting the same service. | A service review reveals repeated degradations over a selected period. |

