# ADR-015: Build vs Buy Decisions

**Date:** 2026-04-15  
**Status:** Proposed  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

Developing a full-stack observability platform is a massive undertaking. We must decide which components are core to our unique value proposition (and thus must be built in-house) and which components can be outsourced to established vendors or open-source solutions to accelerate time-to-market and reduce maintenance overhead.

## Decision

The platform will **Build core observability signals** and **Buy/Leverage specialized supporting services**:
- **Build (In-house):**
  - OTLP Ingest Gateway
  - Unified Query Engine (DataFusion-based)
  - Trace/Log Storage (ClickHouse-based)
  - Observability UI/UX
  - Alert/SLO Evaluation
- **Buy/Leverage (External):**
  - Identity & Auth (Auth0, Okta, or Keycloak)
  - Billing & Metering (Stripe or specialized billing platform)
  - Incident Management (PagerDuty or OpsGenie integrations)
  - Cloud Infrastructure (Managed K8s, Object Storage)

## Consequences

**Easier:** 
- Faster time-to-market for the core observability features.
- Reduced engineering burden for non-core features (billing, SSO).
- Higher reliability for specialized functions (auth, billing).

**Harder:** 
- Increased cost of external vendor dependencies.
- Integration complexity between internal services and external vendors.

**Constrained:** 
- Dependency on external vendors for critical non-core features.

## Alternatives Considered

### Option A: Build Everything In-House
Rejected as it would significantly delay time-to-market and divert resources from core observability innovation.

### Option B: Pure Build on Open Source (e.g., self-hosted billing)
Rejected due to high maintenance overhead and the sensitivity of these specialized functions.

## Related

- `spec/10-process.md` (ADR list)
- `spec/13-risks-roadmap.md` (Final Recommendation)
