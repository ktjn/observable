---
name: tla
description: Formally verify a capability's invariants with TLA+ and TLC, and record the result as metamodel Evidence. Use when the constitution's formal verification assessment flags a component (concurrency, distributed state, financial transactions, safety-critical logic, complex state machines, protocols).
---

# TLA+ Formal Verification

Rules: [constitution § Formal Verification](../../../CONSTITUTION.md#formal-verification). Format: [`spec.md`](../../../spec.md) §§ 4, 8.

## Inputs

- Target capability: `preconditions`, `postconditions`, `invariants`, `failure_modes`.
- Requirement(s) to verify, with `criteria`.
- Consistency boundary of the owning component.

## Steps

1. **Model**: one module per capability, e.g. [`Inventory.tla`](../../../examples/greenfield/Inventory.tla) (config: [`Inventory.cfg`](../../../examples/greenfield/Inventory.cfg)).
   - `VARIABLES`: only state named by the contract.
   - `Init`, one action per operation; guard actions with `preconditions`; model `failure_modes` as explicit actions or outcomes.
   - Model concurrency and retries at the component's consistency boundary.
2. **Properties**: one per contract clause, named after it.
   - `invariants` → `INVARIANT` (e.g. `StockNeverNegative == stock >= 0`).
   - `postconditions` → action properties (e.g. `[][Reserve => stock' = stock - qty]_vars`).
   - Liveness only when a requirement demands it; state fairness explicitly.
3. **Config** (`.cfg`): small constants, explicit bounds; record them as assumptions.
4. **Check**: `java -cp tla2tools.jar tlc2.TLC -config Inventory.cfg Inventory.tla`.
5. **Review**: confirm the model corresponds to the contract and implementation; on violation, fix spec or design, never silently weaken the property.

## Evidence

```yaml
evidence:
  - id: ev-reserve-tla
    requirement_id: req-reserve
    requirement_version: "1.0"
    method: model-check
    bounds: { products: 2, max_qty: 3 }
    passed: true
    provenance: tla+::Inventory.tla
```

- `provenance` identifies the model; keep the `.cfg`, TLC version, bounds, assumptions and output with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- A bounded TLC run is a `model-check` (`bounds` required), not an unbounded proof (`method: proof`).
- Contract changes invalidate the evidence; rerun after `--impact <capability>`.

## Prompt

"Write a TLA+ model of `<capability>` from its contract; one named property per invariant/postcondition; give the `.cfg` with bounds and list assumptions."
