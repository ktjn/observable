---
name: ddd
description: Apply strategic Domain-Driven Design (event storming, bounded contexts, context mapping, aggregates) to discover capabilities, invariants, components and domains. Use in greenfield phases 1–4, brownfield reverse discovery steps 3–4 and modernization re-derivation; not for tactical code design inside a component.
---

# Domain-Driven Design

Rules: [constitution § Components](../../../CONSTITUTION.md#components). Format: [`spec.md`](../../../spec.md) §§ 2–5.

## Mapping

| DDD | Metamodel |
|---|---|
| Bounded context | `system.domains` entry; `Component.domain` |
| Context map relation (customer/supplier, conformist) | `Domain.uses` (acyclic, `DOM-02`, `DOM-03`) and capability `requires` |
| Anti-corruption layer, open host service | Component providing a technology-independent capability over an `external` component |
| Shared kernel | Shared data ownership: a `shared-data-ownership` waiver below release, not a target |
| Aggregate | Consistency boundary: `Component.consistency: strong`, its `owns_data` |
| Aggregate invariant | `capabilities.invariants` |
| Command | Capability with `preconditions`, `postconditions`, `failure_modes` |
| Domain event | Capability with `interface.mode: async`; payload as a `DataModel` |
| Policy (when X then Y) | Requirement on the reacting capability |
| Ubiquitous language | Capability, invariant, data model and field IDs |

Tactical patterns (entities, value objects, repositories) stay inside the component's implementation; they are not specified.

## Steps

1. **Event storming**: list domain events, the commands causing them and the policies reacting to them → capability stubs and failure modes. Brownfield: derive events from writes, messages and logs; record observed behavior only.
2. **Invariants**: for each command, the rules that must hold before and after → `preconditions`, `postconditions`, `invariants`.
3. **Aggregates**: group commands sharing an invariant that needs strong consistency → one component each; invariants spanning components become eventual and need a requirement.
4. **Contexts**: group components sharing a language → `domains`; record relations as `uses`; a cycle means a missing context or misplaced component.
5. **Validate**: `metamodel.py <spec> --stage architecture`.

## Rules

- Names come from the domain, not technology (no `kafka-consumer` component).
- A renamed ID is a breaking change; in a baseline import, declare it (`id_prefix`, `allow_fields`, `refinements`).
- Brownfield: map as-is contexts first; record leaks (shared tables, cycles) as waivers before proposing target contexts ([MODERNIZATION.md](../../../MODERNIZATION.md)).

## Prompt

"Run an event storm of `<domain>`: list events, commands, policies and the invariants each command protects; group commands into aggregates by strong-consistency invariant and aggregates into bounded contexts with their relations. Map to capabilities, components and domains; do not name technologies."
