# Observability — Investigation and diagnosis

**Version:** 1.0 · **Status:** Draft · **Scope:** Business requirements only

## 9. Investigation and diagnosis

**Business intent:** Reduce effort required to explain new or unexpected failures.

| ID | Priority | Requirement | Business acceptance example |
|---|---|---|---|
| OBS-09-01 | P0 | Authorized users shall be able to investigate an incident from the affected service and timeframe. | An investigator can begin from a service status and narrow the failure period. |
| OBS-09-02 | P0 | The platform shall make related operational evidence discoverable in the same investigation context. | An investigator can locate relevant changes, failures, and dependencies without reconstructing context manually. |
| OBS-09-03 | P0 | Investigators shall be able to compare healthy and degraded operating periods. | A known deviation can be identified against a comparable baseline. |
| OBS-09-04 | P1 | Users shall be able to isolate affected operations, consumers, and business segments where that evidence exists. | A partial failure can be separated from unaffected operations. |
| OBS-09-05 | P1 | The platform shall identify plausible contributing factors and distinguish evidence from hypotheses. | An investigation clearly labels unconfirmed cause suggestions. |
| OBS-09-06 | P1 | Investigation findings shall be shareable with other authorized responders. | Two responders can refer to the same saved investigation context. |
| OBS-09-07 | P2 | Users shall be able to compare incidents to identify recurring patterns. | A reviewer can find prior incidents with similar symptoms and confirmed causes. |

