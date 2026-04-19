# ADR-010: Deployment Model (k8s-first)

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The platform is composed of multiple distributed services (ingest gateways, processors, query engines, storage clusters). We need a unified and robust orchestration system that handles scaling, service discovery, high availability, and configuration management.

## Decision

The platform will use a **Kubernetes-first deployment model**.
- All services will be packaged as containers.
- **Helm v3** is the chosen configuration and deployment management tool (Kustomize deferred — see ADR-020).
- We will target standard Kubernetes environments (e.g., EKS, GKE, AKS, and bare-metal distributions like RKE2).
- Operators (e.g., ClickHouse Operator, Strimzi/Redpanda Operator) will be used to manage complex stateful components in production; **kind** is used for local Kubernetes integration testing (see ADR-020).
- Docker Compose remains the canonical local development environment; Helm targets CI/staging/production environments.

## Consequences

**Easier:** 
- Standardized scaling and high availability.
- Portability across cloud providers and on-premise.
- Large ecosystem of tools for observability, security, and GitOps.
- Consistent development and production environments.

**Harder:** 
- Managing Kubernetes clusters adds overhead.
- Complexity in networking and storage configuration (especially for stateful workloads).

**Constrained:** 
- Non-containerized deployments are not natively supported and will require custom engineering if needed.

## Alternatives Considered

### Option A: Serverless/FaaS Only
Rejected due to high cost and unpredictable performance for steady-state high-volume ingestion and complex query workloads.

### Option B: Bare-Metal/VMs with Custom Orchestration
Rejected because Kubernetes provides a standard and more flexible orchestration layer with less custom-built maintenance.

## Related

- `spec/10-process.md` (ADR list)
- `spec/12-deployment.md` (Deployment Strategy)
- `spec/13-risks-roadmap.md` (Final Recommendation)
- `ADR-020`: Helm Chart Strategy (library + umbrella chart, kind for testing)
