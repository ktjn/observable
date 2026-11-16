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
6. **Correspondence**: a passing `check` says the *structure* holds, not that the code builds that structure ([`VERIFIED-DEVELOPMENT.md` §5](../../../VERIFIED-DEVELOPMENT.md)). Tie them with at least one of:
   - using Alloy instances (and the Evaluator's answers) as test fixtures: build each instance in the implementation and assert the predicates hold, and that states the model forbids are rejected;
   - a differential test of the implementation against an executable form of the predicates over generated data;
   - an abstraction function from implementation state to model `sig`s, reviewed against each operation.
   Record each as `test` (or `analysis`) evidence bound to the same requirement (`verifies`, [`spec.md` § 8](../../../spec.md#8-verification)). Without one, the requirement is verified by the model only: `--evidence-report` lists it with `model_only: true`.

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
    artifacts: { Inventory.als: <sha256>, alloy.summary: <sha256> }
```

- `provenance` identifies the model.
- Hash in `artifacts` (SHA-256, `sha256sum`) the `.als` model (scopes and bitwidth are in its `check` commands) and a normalized **result summary**, not the raw log (`alloy.summary`): the Alloy version and one line per command with its scope and verdict (no counterexample, or counterexample found), produced by a small script over the solver output and sorted by command. The verdict of an exhaustive `check` at a fixed scope does not vary; solver, time and path lines do; editing any of them makes `--check-artifacts` fail and name the record. Keep the Alloy and solver versions, scopes and bitwidth and assumptions with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- Keep the raw log out of the repository or unhashed (git-ignored): it carries pids, timings and absolute local paths and changes on every run, so its hash proves only that the file was not edited, not that it came from the hashed model.
- Rerun in CI when the configuration is fast: regenerate the summary from the hashed model and fail on any difference from the committed one (`diff`), so a model edit cannot pass by updating hashes and a mismatch is a real failure, not noise. Slow configurations stay manual and pinned by hash only: record them as such (`verifier`, `timestamp`) and rerun after any change.
- A correspondence check is `test` evidence, not part of the `model-check` record.
- A bounded `check` is a `model-check` (`bounds` required, e.g. the scope), not an unbounded proof (`method: proof`).
- Contract changes invalidate the evidence; rerun after `--impact <capability>`.

## Prompt

"Write an Alloy model of `<capability>` from its contract; one named `assert` per invariant/postcondition and a `run` per operation; give scopes and list assumptions."
