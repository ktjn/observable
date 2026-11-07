# ADR-039: Health Assessment as a Domain Capability

**Date:** 2026-11-07  
**Status:** Accepted (2026-11-07; decisions Q1–Q10 in docs/plans/business-spec-adoption.md, accepted by merging the 2.0 re-review PR)  
**Authors:** Claude Code  
**Deciders:** Tommy (ktjn)  
**Superseded by:**  
**Review date:** 2026-12-07  

## Context

Health is computed at query time in `services/query-api/src/discovery.rs` from error rate alone, with fixed 1%/5% thresholds; a service with no spans is reported `healthy`. The specification requires normal, degraded, unavailable, planned maintenance and unknown states, last-evidence time, staleness and data-quality signals (OBS-03, OBS-26, OBS-29-03), with business-owned thresholds, and names `risk-false-healthy`.

## Decision

Add a domain-layer component `observable-health` in domain `service-assessment` providing `assess-service-health`. `assess-customer-experience` and `track-business-processes` are not assigned here: the same context-budget argument that keeps health out of alerting applies to health itself, so their provider and domain are decided in the Phase 7 detailed plan. It evaluates configurable health rules per catalog service, records the evaluated state with its evidence age and contributing conditions, and treats absent or stale evidence as `unknown`. Query, UI and alerting read the evaluated state; no consumer derives health on its own. Health reads go through `decide-access` (`req-health-access-control`): restricted evidence is withheld without reporting the service as normal.

## Consequences

**Easier:** One health semantics across overview, alerting, SLOs and reports; `unknown` is explicit; thresholds are configuration.  
**Harder:** A stateful evaluator with its own schedule and storage; evaluation lag must be disclosed.  
**Constrained:** The `healthy | watch | breach` API is removed on `2.0`.

## Alternatives Considered

### Option A: Fix the query-time helper
Smallest change, but cannot hold maintenance windows, rules or history. Rejected; the user also chose not to patch it standalone (Q6).

### Option B: Fold health into observable-alerting
Alerting already evaluates rules, but health is consumed far more widely and would push alerting past its context budget. Rejected.

## Related

- [docs/plans/business-spec-adoption.md](../../docs/plans/business-spec-adoption.md) section 4 decision 3
- [ADR-038](ADR-038-service-catalog-component.md)
