# Replacement: Building an Alternative Implementation

**Optional development methodology.** This workflow applies to a bounded scope whose behavioral specification has been reviewed and accepted. It does not extend the metamodel or impose a universal release policy. See [MODEL-SCOPE.md](MODEL-SCOPE.md).

## Goal

Build an implementation that satisfies an accepted specification **without relying on the old implementation as the source of behavioral authority**. The replacement may change languages, algorithms, physical architecture, source layout or deployment strategy within applicable adopted constraints.

Replacement is distinct from feature development: required observable behavior should remain unchanged unless a separate specification change is reviewed. Passing tests is evidence only for the checks, assumptions, bounds and conditions exercised—not a proof of complete equivalence.

## Prerequisites

- An accepted specification for a precisely bounded system, component or capability scope, with pinned capability/requirement/contract versions.
- Applicable risks, mitigations, quality expectations, external contracts, data semantics and verification obligations identified.
- An explicit register of unspecified behavior, assumptions, coverage limits and known characterization gaps.
- Project-defined acceptance policy and an independent verification mechanism.
- A reviewed snapshot of the existing implementation only if a comparison or migration is required; the implementation agent does not need access to its internals.

A brownfield specification-authority decision (see [BROWNFIELD.md](BROWNFIELD.md)) establishes the accepted baseline for the adopted slice. It is not a declaration of whole-system completeness or release conformance.

## Component replacement without dependent implementations

Under the optional [component isolation methodology](COMPONENT-ISOLATION.md), a replacement candidate must run independently using substituted required capabilities. Consumers may continue development with the provider's published programmable mock while the real provider is replaced. The replacement and published mock must satisfy the applicable accepted contract verification; integration is assessed separately. Do not reuse evidence from the old implementation as proof of the new candidate.

## Workflow

| Step | Activity | Output / decision |
|---|---|---|
| 1. Freeze | Pin the accepted behavioral specification, risks, external contracts, data semantics and verification assets. | Immutable baseline and explicit scope. |
| 2. Assess adequacy | Review requirements for observable outcomes, failures, concurrency, state transitions, non-functional expectations and assumptions; record gaps rather than claiming exhaustiveness. | Accepted obligations and unresolved questions. |
| 3. Plan | Identify logical components, required capabilities, dependent consumers and integration points. Use `--impact` and `--plan` where useful. | Bounded implementation handoffs. |
| 4. Build | Give implementation agents specification and required external interfaces; omit old implementation internals when testing independence. | Alternative implementation and newly discovered questions. |
| 5. Verify | Run independent checks against the candidate revision: component, contract, integration, property, fault and/or formal checks as applicable. | Requirement-linked evidence with subject revision, provenance, conditions and limits. |
| 6. Compare | Assess observable differences, compatibility, migration/cutover behavior and risk exposure. Baseline differential tests are useful but never silently elevate old bugs into normative contracts. | Conformance findings and migration plan. |
| 7. Decide | Accept, rework, or reopen specification review. Only the consuming project's authorized decision can change behavioral obligations. | Scoped acceptance decision and retained evidence. |

## Decision Rules

- **Implementation defect:** the replacement violates an accepted obligation. Fix the implementation; do not edit the specification to manufacture a pass.
- **Specification gap or ambiguity:** intended observable behavior cannot be derived from the accepted specification. Reopen the relevant requirement/contract/risk review; add verification obligations before relying on new behavior.
- **Observed legacy discrepancy:** the old implementation differs from the accepted specification. Decide explicitly whether to correct, preserve through a reviewed contract change, or handle through migration policy.
- **Unconstrained behavior:** multiple implementations behave differently without violating any accepted obligation. Do not demand byte-for-byte or internal structural equivalence without a requirement.
- **Insufficient verification:** passing checks do not cover an applicable obligation or rely on unsupported assumptions. Retain an open verification gap; do not claim conformance.
- **Behavioral change requested:** use [FEATURE.md](FEATURE.md) to revise and version the specification before accepting altered contracts.

## Evidence and Handoff

For each candidate implementation record the accepted spec revision, candidate source/build revision, verification tool and version, execution environment, applicable assumptions and bounds, test/proof artifacts, failures/skips and assessment of residual risks. Use the existing `subject_bindings`, `Evidence`, `--check-report`, `--check-return`, `--check-artifacts` and `--compat` mechanisms where suitable. These checks have documented limits; independent review and native gates remain necessary.

The implementation agent should report missing or contradictory specifications, hidden coupling and unsupported environmental assumptions. Preserve failed results as history rather than replacing them silently.

## Recommended Experiment

For an accepted component:

1. Give a fresh agent only the specification, external contracts and verification obligations.
2. Ask for an implementation in a different language or with a different architecture.
3. Verify it independently without using implementation-specific tests as normative contracts.
4. Classify every failure as an implementation defect, missing/ambiguous requirement, verification gap or intentionally unconstrained behavior.
5. Update the specification or its verification assets through reviewed changes, not by copying accidental implementation details.

The experiment tests whether the accepted specification contains enough durable behavioral knowledge to construct a conforming implementation. It cannot prove exhaustive behavioral completeness.

## Relationship to Other Tracks

- [GREENFIELD.md](GREENFIELD.md): derives specifications from intended outcomes.
- [BROWNFIELD.md](BROWNFIELD.md): recovers specifications from observed existing systems and establishes bounded authority decisions.
- [FEATURE.md](FEATURE.md): changes intended observable behavior against an accepted specification.
- **REPLACEMENT.md:** changes an implementation while preserving accepted obligations.
- [MODERNIZATION.md](MODERNIZATION.md): questions mechanisms of an onboarded system and routes the next generation to FEATURE.md or REPLACEMENT.md.

All five tracks are optional methodologies. The [metamodel](spec.md) remains platform- and implementation-independent.
