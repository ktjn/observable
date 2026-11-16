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
6. **Correspondence**: a passing TLC run says the *design* holds, not that the code matches it ([`VERIFIED-DEVELOPMENT.md` §5](../../../VERIFIED-DEVELOPMENT.md)). Tie them with at least one of:
   - replaying model behaviours against the implementation (TLC `-simulate` with `-dump`, or a simulator that emits traces);
   - validating implementation logs or traces against the spec (trace validation);
   - a refinement argument: an abstraction function from implementation state to model state (a TLA+ refinement mapping for a later design).
   For a refactoring, also model the as-is design first and check the new one against it with a refinement mapping; replay the same traces against old and new code ([`BROWNFIELD.md`](../../../BROWNFIELD.md#models-as-a-safety-net-for-refactoring)).
  Record each as `test` (or `analysis`) evidence bound to the same requirement (`verifies`, [`spec.md` § 8](../../../spec.md#8-verification)). Without one, the requirement is verified by the model only: `--evidence-report` lists it with `model_only: true`.

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
    artifacts: { Inventory.tla: <sha256>, Inventory.cfg: <sha256>, tlc.summary: <sha256> }
```

- `provenance` identifies the model.
- Hash in `artifacts` (SHA-256, `sha256sum`) the `.tla` model, the `.cfg` and a normalized **result summary**, not the raw log (`tlc.summary`): the version line, `Model checking completed. No error has been found.`, `M distinct states found, 0 states left on queue.` and `The depth of the complete state graph search is D.`. For example `tlc ... | tee tlc.log | grep -E '^(TLC2 Version|Model checking completed|[0-9]+ states generated|The depth of the complete)' | sed -E 's/^[0-9]+ states generated, //' > tlc.summary`. An exhaustive run gives the same verdict, distinct-state count and depth every time; editing any of them makes `--check-artifacts` fail and name the record. Keep the TLC version and assumptions with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- Keep the raw log out of the repository or unhashed (git-ignored): it carries pids, timings and absolute local paths and changes on every run, so its hash proves only that the file was not edited, not that it came from the hashed model.
- Rerun in CI when the configuration is fast: regenerate the summary from the hashed model and fail on any difference from the committed one (`diff`), so a model edit cannot pass by updating hashes and a mismatch is a real failure, not noise. Slow configurations stay manual and pinned by hash only: record them as such (`verifier`, `timestamp`) and rerun after any change. Simulation (`-simulate`) and any random walk are not reproducible this way unless the seed and depth are fixed; keep them as `analysis` with those recorded.
- A correspondence check is `test` evidence, not part of the `model-check` record.
- A bounded TLC run is a `model-check` (`bounds` required), not an unbounded proof (`method: proof`).
- Contract changes invalidate the evidence; rerun after `--impact <capability>`.

## Prompt

"Write a TLA+ model of `<capability>` from its contract; one named property per invariant/postcondition; give the `.cfg` with bounds and list assumptions."
