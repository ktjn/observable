# ADR-011: Sampling Strategy

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

Storing 100% of telemetry data at high scale is economically and technically prohibitive. Sampling is required to manage storage costs and query performance while maintaining enough signal for alerting, troubleshooting, and trend analysis.

## Decision

The platform will implement a **multi-layered sampling strategy**:
1.  **Head Sampling:** Performed at the agent or ingest gateway based on simple probabilistic rules.
2.  **Tail Sampling:** Performed after trace spans are buffered, allowing for decision-making based on the complete trace (e.g., keep all traces with errors or high latency).
3.  **Adaptive Sampling:** The platform will dynamically adjust sampling rates based on tenant-specific cardinality budgets and total system load.

## Consequences

**Easier:** 
- Controlled storage costs and better query performance.
- High-fidelity data for critical events (errors, latency spikes).
- Protection against "telemetry storms."

**Harder:** 
- Tail sampling requires significant buffering and state management.
- More complex to explain to users (potential for "missing" data).
- Ensuring statistical significance for metrics derived from sampled data.

**Constrained:** 
- Users must accept that not all data is stored at full fidelity.

## Alternatives Considered

### Option A: Fixed-Rate Sampling
Rejected because it often misses rare but critical events like errors or tail-latency spans.

### Option B: No Sampling (Store Everything)
Rejected due to prohibitive cost and scalability issues for large-scale production workloads.

## Related

- `spec/06-agents.md` (Agent Functions: adaptive sampling)
- `spec/10-process.md` (ADR list)
- `spec/13-risks-roadmap.md` (Risk 2: Cardinality collapse)
