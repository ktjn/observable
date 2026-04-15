# ADR-004: Rust for Data Plane Services

**Date:** 2026-04-15  
**Status:** Proposed  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The data plane (ingest, processing, query) must handle extremely high throughput with low latency and predictable resource consumption. Memory safety and concurrency are critical to prevent crashes and security vulnerabilities in the hot path.

## Decision

The **data plane (ingest gateways, stream processors, query executors) will be written in Rust**. This aligns with the modern observability ecosystem (e.g., OTel Collector, DataFusion, Polars).

## Consequences

**Easier:** 
- Exceptional performance and low memory footprint.
- Strong memory safety guarantees (no GC pauses).
- Excellent support for gRPC, HTTP/2, and async IO (Tokio).
- Direct integration with Apache Arrow and DataFusion.

**Harder:** 
- Steeper learning curve compared to languages like Go or Python.
- Slower compile times.
- Stricter development discipline required (borrow checker).

**Constrained:** 
- Hiring and onboarding require focus on developers with Rust experience or the willingness to learn it.

## Alternatives Considered

### Option A: Go
Rejected because while Go is efficient, Rust provides better control over memory and zero-cost abstractions, which are critical for high-throughput data processing and custom query engines.

### Option B: C++
Rejected due to lack of memory safety guarantees and a more fragmented ecosystem compared to modern Rust.

## Related

- `spec/10-process.md` (ADR list)
- `spec/13-risks-roadmap.md` (Final Recommendation)
- `spec/03-storage.md` (Query Engine)
