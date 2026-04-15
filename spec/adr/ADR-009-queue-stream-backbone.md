# ADR-009: Queue/Stream Backbone

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

High-throughput telemetry ingestion can be unpredictable, with bursts and backpressure. Ingesting data directly into storage engines can lead to data loss or performance degradation. A durable buffering layer is essential for reliability, scalability, and asynchronous processing.

## Decision

The platform will use a **distributed, durable message queue (e.g., Redpanda or Kafka)** as its ingestion backbone. 
- All incoming OTLP data will be written to the queue before being consumed by downstream processors (storage writers, enrichment engines, alert evaluators).
- This provides "at-least-once" delivery and allows for independent scaling of ingest and storage.

## Consequences

**Easier:** 
- Resilience to storage engine outages or slow-downs.
- Decoupling of ingest throughput from storage write performance.
- Enables asynchronous processing (enrichment, materialization).
- Unified buffer for all telemetry signals.

**Harder:** 
- Increased operational complexity (managing a message queue cluster).
- Added latency between ingestion and data availability for querying.
- Must manage consumer group offsets and handle potential re-processing.

**Constrained:** 
- The system must handle duplicate data (idempotent writes to storage).

## Alternatives Considered

### Option A: Direct-to-Storage Ingestion
Rejected because it lacks buffering and resilience during high-load scenarios or storage maintenance.

### Option B: Redis Streams
Rejected due to lower durability guarantees and smaller scaling limits compared to Kafka/Redpanda for high-volume telemetry.

## Related

- `spec/02-architecture.md` (Ingestion)
- `spec/03-storage.md` (ClickHouse insertion buffering)
- `spec/10-process.md` (ADR list)
