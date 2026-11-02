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

## Use in AI-driven development

AI-driven development is one supported application, not an intrinsic dependency. A project can derive bounded work scopes, dependency context, contracts, required evidence and traceability from the model. Scheduling, agent isolation, source ownership, independent review, context budgets, fake dependencies and acceptance gates are project/methodology decisions, not mandatory metamodel properties.

## Real-world adoption

Onboarding records in this repository describe applying the model to [Modelable](archive/feedback/modelable-onboarding.md), [Observable](archive/feedback/observable-onboarding.md), [Scalable](archive/feedback/scalable-onboarding.md) and a marketplace system ([record](archive/feedback/marketplace-model.md)). These demonstrate distinct architecture and adoption scopes; each project's own specification and records remain authoritative for how it applies the model. They are examples, not a universal conformance suite.

## Evolution principle

Refine the conceptual model based on cross-project experience. Keep universal semantics distinct from optional policy. Do not add a mandatory model constraint merely because the reference implementation can enforce it.
