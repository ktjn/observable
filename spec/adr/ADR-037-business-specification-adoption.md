# ADR-037: Business Specification as the 2.0 Requirement Baseline

**Date:** 2026-11-07  
**Status:** Proposed  
**Authors:** Claude Code  
**Deciders:** Tommy (ktjn)  
**Superseded by:**  
**Review date:** 2026-12-07  

## Context

The vendor-neutral observability business specification (v2.5: 293 requirements, 37 areas, UX-01..UX-10, 15 risks) defines what Observable must become. `spec/system.yaml` held 17 recovered technical requirements. Both use the system-spec metamodel (`system-spec-ref 0.5.0`). Decisions Q1, Q2, Q7, Q8, Q9 and Q10 in [docs/plans/business-spec-adoption.md](../../docs/plans/business-spec-adoption.md) were taken on 2026-11-07.

## Decision

Merge the business specification into `spec/system.yaml` (via `include` fragments under `spec/business/`), retarget each `OBS-*` requirement to an Observable capability while keeping its ID, and develop it on a long-lived `2.0` branch as a new start that allows breaking changes. Validate at `--stage verification` with a P0 readiness report until `2.0.0`; the upstream specification repository is retired after the merge.

## Consequences

**Easier:** One spec, one validator run, one impact analysis; requirements, workflows and risks resolve against real components and capabilities; progress is measurable per phase (`scripts/spec-readiness.py`).  
**Harder:** `--stage release` fails until every active requirement is verified, so it cannot guard regressions on the 2.0 line until `2.0.0`; per-requirement gap status lives outside the metamodel (`docs/plans/business-spec-gap.yaml`).  
**Constrained:** No compatibility, migration or data preservation from 0.x on `2.0`. Area and holder are not metamodel fields; they live in fragment file names and the gap file. `state_sets` stays in the root file until OBSSPEC-EXP-09 is fixed upstream.

## Alternatives Considered

### Option A: External pinned spec plus traceability map
Keeps layers apart and the release gate green, but needs a second validator run and a custom checker. Rejected by the decider in favour of a single spec.

### Option B: Second spec root vendored in the repo
Avoids retargeting but leaves two specs that can drift. Rejected.

### Option C: Merge phase by phase
Keeps only the current phase failing, but hides the full scope from impact analysis. Rejected for the verification-stage gate.

## Related

- [ROADMAP.md](../../ROADMAP.md)
- [spec/22-system-specification.md](../22-system-specification.md)
- [ADR-035: Component independence](ADR-035-component-independence.md)
