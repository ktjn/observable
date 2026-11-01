---
name: alloy
description: Formally verify a capability's structural invariants with Alloy and record the result as metamodel Evidence. Use when the constitution's formal verification assessment flags a component whose risk lies in data models, relations, references, permissions or configuration rather than interleaving (prefer the tla skill for concurrency and protocols).
---

# Alloy Formal Verification

Rules: [constitution § Formal Verification](../../../CONSTITUTION.md#formal-verification). Format: [`spec.md`](../../../spec.md) §§ 4, 8.

## Inputs

- Target capability: `preconditions`, `postconditions`, `invariants`, `failure_modes`.
- Requirement(s) to verify, with `criteria`.
- Entities and relations owned by the component.

## Steps

1. **Model**: one module per capability, e.g. [`Inventory.als`](../../../examples/greenfield/Inventory.als) (metamodel example: [`metamodel.als`](../../../reference-implementation/metamodel.als)).
   - `sig`s and fields: only entities and relations named by the contract.
   - `fact`s: structural rules the system enforces; never encode properties under check as facts.
   - One `pred` per operation over pre/post state (`s, s'` or Alloy 6 `var` fields); guard with `preconditions`; model `failure_modes` as explicit outcomes.
2. **Properties**: one `assert` per contract clause, named after it.
   - `invariants` → `assert StockNeverNegative { all s: State | s.stock >= 0 }`.
   - `postconditions` → `assert ReservePost { all s, s': State, q: Int | reserve[s, s', q] => s'.stock = minus[s.stock, q] }`.
   - One `run` per operation to show it is satisfiable (guards against vacuous checks).
3. **Scope**: e.g. `check StockNeverNegative for 5 but 4 Int`; small, explicit scopes and bitwidth; record them as assumptions.
4. **Check**: `java -jar org.alloytools.alloy.dist.jar exec Inventory.als`.
5. **Review**: confirm the model corresponds to the contract and implementation; on counterexample, fix spec or design, never silently weaken the assertion or add a fact to hide it.

## Evidence

```yaml
evidence:
  - id: ev-reserve-alloy
    requirement_id: req-reserve
    requirement_version: "1.0"
    method: model-check
    bounds: { scope: 5 }
    passed: true
    provenance: alloy::Inventory.als
```

- `provenance` identifies the model; keep scopes, bitwidth, solver, Alloy version, assumptions and output with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- A bounded `check` is a `model-check` (`bounds` required, e.g. the scope), not an unbounded proof (`method: proof`).
- Contract changes invalidate the evidence; rerun after `--impact <capability>`.

## Prompt

"Write an Alloy model of `<capability>` from its contract; one named `assert` per invariant/postcondition and a `run` per operation; give scopes and list assumptions."
