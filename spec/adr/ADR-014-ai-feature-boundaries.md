# ADR-014: AI Feature Boundaries

**Date:** 2026-04-15  
**Status:** Proposed  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

AI/ML features are high-value but can be opaque and unreliable if not grounded in high-quality data. Over-prioritizing AI before the foundational observability platform is stable and scaled is a significant risk.

## Decision

The platform will follow a **"Ship AI Late" strategy**. 
- Foundational observability (Phase 0–4) must be solid before Phase 7 (Intelligence) features are broadly introduced.
- AI will be used primarily as an advisory layer (summarization, root-cause candidate ranking) rather than an automated decision-maker.
- No auto-remediation will be performed without explicit policy gates and human-in-the-loop approvals.

## Consequences

**Easier:** 
- Clear focus on platform reliability and data fidelity.
- Avoids the "AI-first fantasy" trap.
- Better quality AI outputs because the underlying data is reliable.

**Harder:** 
- May be perceived as "lagging" in AI features compared to competitors.
- Delay in delivering high-value automation features.

**Constrained:** 
- AI features are advisory only in the initial release phases.

## Alternatives Considered

### Option A: AI-First (Build around LLM/ML from day 1)
Rejected as it risks building on an unstable foundation and potentially delivering inaccurate or misleading results.

### Option B: Deep AI Integration (e.g., AI-only query layer)
Rejected because human-readable and deterministic query results are essential for production observability.

## Related

- `spec/08-ai-ml.md` (AI/ML Features)
- `spec/10-process.md` (Phase 7: Intelligence)
- `spec/13-risks-roadmap.md` (Risk 6: AI-first roadmap)
