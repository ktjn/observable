# Modernization: Questioning Mechanisms and Proposing a Next Generation

**Optional development methodology.** Applies to a brownfield scope with a recorded specification authority decision ([BROWNFIELD.md](BROWNFIELD.md#convergence-from-code-assisted-discovery-to-specification-controlled-development)). Artifact rules: [constitution](CONSTITUTION.md). It adds no metamodel fields; see [MODEL-SCOPE.md](MODEL-SCOPE.md).

## Goal

Make the algorithm and technology choices of an onboarded system explicit, question them against accepted obligations, and derive a next-generation proposal the way a greenfield architecture is derived.

- The metamodel prescribes no implementation mechanisms ([`spec.md` § 9](spec.md#9-scalability-and-quality)), so brownfield discovery recovers *what* the system does but not *why* it is built this way.
- A mechanism (algorithm, data structure, storage engine, protocol, framework, deployment topology) is justified only by the requirements, invariants, risks and quality attributes it serves.
- A mechanism that serves none is accidental: it is replaceable without a specification change.

## Prerequisites

- Accepted capabilities, invariants and data models for the scope (Level 2 or later); they correspond to greenfield phases 1–2 and 5.
- Measured as-is quality: performance, scalability and resilience requirements with `analysis` or `test` evidence. A waived category (`waives: verification-category`) cannot justify or reject a mechanism; characterize it first.

## Steps

| # | Step | Output | Check |
|---|---|---|---|
| 1 | Inventory mechanisms | One [decision record](templates/MODERNIZATION-DECISION.md) per material mechanism, bound to components and source revision | Every component in scope has its mechanisms listed or `none material` |
| 2 | Recover forces | Requirements, invariants, risks and quality attributes each mechanism serves; missing quality requirements added as `specified` | No force without a requirement or invariant ID; unlinked mechanisms marked accidental |
| 3 | Question | Alternatives per mechanism, with trade-offs against the same forces | Assumptions behind the original choice re-checked (load, data volume, team, platform, cost) |
| 4 | Evaluate | Spikes, benchmarks and formal models for risky alternatives ([`tla`](.github/skills/tla/SKILL.md), [`quint`](.github/skills/quint/SKILL.md), [`alloy`](.github/skills/alloy/SKILL.md)) | Evidence binds to the spike's revision; it is not evidence for the product |
| 5 | Decide | `keep`, `replace`, `investigate` or `retire` per mechanism | Recorded in the [gate decision record](CONSTITUTION.md#gate-decision-record) |
| 6 | Re-derive architecture | Next-generation components from the accepted invariants and consistency boundaries ([GREENFIELD.md](GREENFIELD.md) phases 3–6, 9–10; [planned components](GREENFIELD.md#planned-components)); optional [`ddd`](.github/skills/ddd/SKILL.md) skill | `--stage architecture`; diff against the as-is components |
| 7 | Route | Behavior or quality changes → [FEATURE.md](FEATURE.md); implementation-only changes → [REPLACEMENT.md](REPLACEMENT.md), slice by slice | `--compat <as-is>` for capabilities and data models |

## Next-generation specification

Two options:

- **In place:** evolve the accepted spec through [FEATURE.md](FEATURE.md); the as-is revision is `old.yaml`.
- **Separate spec:** import the accepted as-is spec with `system.imports` `mode: baseline` ([GREENFIELD.md § Adopting an upstream specification](GREENFIELD.md#adopting-an-upstream-specification)). Requirements and risks stay the baseline; every change or removal is a `refinement` whose `decision` names the mechanism decision record. Components are re-derived, not imported. Check with `--check-imports --root` and `--compat <as-is>`.

## Rules

- Question mechanisms, not obligations. Changing an accepted requirement is a [FEATURE.md](FEATURE.md) change with its own review.
- Do not promote a mechanism into a contract to protect it. A capability clause naming a technology (e.g. a storage engine) is hidden coupling; record it as [feedback](CONSTITUTION.md#self-improvement-and-model-feedback).
- Keep decisions out of component specifications ([constitution § Components](CONSTITUTION.md#components)); decision records live beside the spec.
- Characterization tests are acceptance tests for the next generation only where they pin `required` behavior; drop or rewrite those pinning accidental behavior ([BROWNFIELD.md § Convergence](BROWNFIELD.md#convergence-from-code-assisted-discovery-to-specification-controlled-development)).
- Implementation follows the test-first loop ([GREENFIELD.md](GREENFIELD.md#implementation-and-acceptance-loop)); unit tests bind to component sub-requirements ([FEATURE.md](FEATURE.md#decompose-and-hand-off)).
- Evidence from the old implementation, spikes or benchmarks of alternatives does not verify the new candidate ([REPLACEMENT.md](REPLACEMENT.md)).

## Gate review

| Gate | Bring to review | Decision required before proceeding |
|---|---|---|
| Forces | Decision records, linked requirements and as-is measurements | Every material mechanism is linked or marked accidental; missing quality requirements are specified |
| Proposal | Alternatives, evaluation evidence, re-derived architecture, `--compat` result | Each `replace` preserves accepted obligations or names the FEATURE.md change it needs; migration order is set |

## Prompts

- **Inventory**: "List material algorithms and technologies in `<component>` with source locations; link each to requirement, invariant, risk or quality attribute IDs; mark unlinked ones accidental. Do not propose alternatives yet."
- **Question**: "For `<mechanism>`, list alternatives and their trade-offs against these forces; state which assumptions of the original choice no longer hold and what evidence would decide."
- **Re-derive**: "Derive components from these accepted invariants and consistency boundaries without reference to the current module layout; diff against the as-is components."
