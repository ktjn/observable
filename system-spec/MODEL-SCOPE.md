# Metamodel scope and authority

System Specification is a platform-independent **metamodel** for describing systems. It is not a mandatory development framework, runtime architecture, compliance regime, or enforcement system.

## Core concepts

- **System:** composition of logical components.
- **Component:** bounded unit of responsibility, independent of source layout or deployment.
- **Capability:** externally meaningful behavior provided or required by a component.
- **Contract:** expected behavior and interaction constraints of a capability.
- **Data model:** owned, consumed, derived or exposed data and its lineage.
- **Requirement:** statement of expected behavior or quality, with acceptance criteria.
- **Risk:** potential undesirable outcome and its treatment.
- **Verification obligation:** what must be established; **evidence:** why a claim is considered satisfied.

The metamodel defines the vocabulary and relationships. It permits projects to describe consistency, layering, ownership, lifecycle, change impact and verification without prescribing a specific implementation.

## Authority boundaries

| Concern | Authority |
| --- | --- |
| Metamodel semantics and relationships | `spec.md` and this scope statement |
| Development principles and workflows | Optional methodologies such as `CONSTITUTION.md`, `GREENFIELD.md`, `BROWNFIELD.md` and `FEATURE.md` |
| Rules to enforce, waivers, acceptance and evidence sufficiency | Consuming project |
| Serialization, diagnostics and supported checks | Reference implementation, illustrative only |

A reference validator result does **not** establish that a project is compliant, correctly implemented, or ready for release. A validation failure does not automatically prohibit adoption: projects determine the applicability of policies and the checks they require. The current reference format and its stage checks represent one possible interpretation and must not silently become universal metamodel constraints.

## Durable specification versus replaceable implementation

The durable intent of a system consists of requirements, risks, capabilities, behavioral contracts, invariants and verification obligations. An implementation is a replaceable candidate realization. Verification evidence is bound to the implementation and conditions actually examined; it is not transferable to another implementation merely because the specification is unchanged.

A replacement can use different code, language, structure or algorithm when the adopted behavioral and quality obligations permit it. A passing finite test suite does not prove that all intended behavior was specified or exercised. Unknown or unspecified behaviors must not be silently inferred from a legacy implementation or promoted to normative requirements.

Brownfield discovery uses existing source and tests as evidence to recover an accepted specification. Once accepted for a bounded scope, subsequent implementation, evolution or replacement is governed by the specification and project policy. Existing code can remain a useful source of observations without being the authoritative behavioral definition.

## Use in AI-driven development

AI-driven development is one supported application, not an intrinsic dependency. A project can derive bounded work scopes, dependency context, contracts, required evidence and traceability from the model. Scheduling, agent isolation, source ownership, independent review, context budgets, fake dependencies and acceptance gates are project/methodology decisions, not mandatory metamodel properties.

## Real-world adoption

Onboarding records in this repository describe applying the model to [Modelable](archive/feedback/modelable-onboarding.md), [Observable](archive/feedback/observable-onboarding.md), [Scalable](archive/feedback/scalable-onboarding.md) and a marketplace system ([record](archive/feedback/marketplace-model.md)). These demonstrate distinct architecture and adoption scopes; each project's own specification and records remain authoritative for how it applies the model. They are examples, not a universal conformance suite.

## Evolution principle

Refine the conceptual model based on cross-project experience. Keep universal semantics distinct from optional policy. Do not add a mandatory model constraint merely because the reference implementation can enforce it.
