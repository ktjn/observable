---
name: quint
description: Verify a capability's invariants with a Quint model (typecheck, simulate, bounded model check) and record the result as metamodel Evidence. Use when the constitution's formal verification assessment flags a component (concurrency, distributed state, financial transactions, safety-critical logic, complex state machines, protocols) and the team prefers a typed, programmer-friendly notation over TLA+.
---

# Quint Verification

Rules: [constitution § Formal Verification](../../../CONSTITUTION.md#formal-verification). Format: [`spec.md`](../../../spec.md) §§ 4, 8. Same problem space as [`tla`](../tla/SKILL.md) (interleavings, protocols, state machines); for structure and relations use [`alloy`](../alloy/SKILL.md). Check flags against `quint --help` for your version.

## Inputs

- Target capability: `preconditions`, `postconditions`, `invariants`, `failure_modes`.
- Requirement(s) to verify, with `criteria`.
- Consistency boundary of the owning component.

## Steps

1. **Model**: one module per capability, e.g. `inventory.qnt`.
   - `var`s: only state named by the contract.
   - `init`, one action per operation; guard actions with `preconditions`; model `failure_modes` as explicit outcomes.
   - A `step` action that nondeterministically picks an operation (`any { ... }`, `nondet`) models concurrency and retries at the component's consistency boundary.
2. **Properties**: one per contract clause, named after it.
   - `invariants` → a `val` (e.g. `StockNeverNegative = stock >= 0`) passed as `--invariant`.
   - `postconditions` → checked inside the action or as a temporal property.
   - Liveness only when a requirement demands it; state fairness explicitly.
3. **Bounds**: small constants and a step limit; record them as assumptions.
4. **Check**, in this order:
   - `quint typecheck inventory.qnt`
   - `quint run inventory.qnt --invariant StockNeverNegative --max-samples <n> --max-steps <k> --seed <s>`: randomized simulation; finds counterexamples, proves nothing.
   - `quint verify inventory.qnt --invariant StockNeverNegative --max-steps <k>`: bounded model check (Apalache); exhaustive up to the bounds.
5. **Review**: confirm the model corresponds to the contract and implementation; on violation, fix spec or design, never silently weaken the property.
6. **Correspondence**: a passing model check says the *design* holds, not that the code matches it ([`VERIFIED-DEVELOPMENT.md` §5](../../../VERIFIED-DEVELOPMENT.md)). Tie them with at least one of:
   - replaying model traces (`--out-itf`) as tests against the implementation;
   - a differential test against the model's executable `step` (`quint run`/`quint compile`);
   - an abstraction function from implementation state to model state, reviewed against each action.
   For a refactoring, also model the as-is design first, check the same properties on the new module and replay the same traces against old and new code ([`BROWNFIELD.md`](../../../BROWNFIELD.md#models-as-a-safety-net-for-refactoring)).
  Record each as `test` (or `analysis`) evidence bound to the same requirement (`verifies`, [`spec.md` § 8](../../../spec.md#8-verification)). Without one, the requirement is verified by the model only: `--evidence-report` lists it with `model_only: true`.

## Evidence

```yaml
evidence:
  - id: ev-reserve-quint
    requirement_id: req-reserve
    requirement_version: "1.0"
    method: model-check
    bounds: { products: 2, max_qty: 3, max_steps: 10 }
    passed: true
    provenance: quint::inventory.qnt
    artifacts: { inventory.qnt: <sha256>, verify.summary: <sha256> }
  - id: ev-reserve-quint-sim
    requirement_id: req-reserve
    requirement_version: "1.0"
    method: analysis
    bounds: { samples: 10000, max_steps: 20, seed: 1 }
    passed: true
    provenance: quint::inventory.qnt
```

- A correspondence check is `test` evidence, not part of the `model-check` record.
- `quint verify` is a `model-check` (`bounds` required, `EV-01`), not an unbounded proof (`method: proof`).
- `quint run` is sampling, not exhaustive: record it as `analysis` (or `test`), never `model-check`.
- Hash in `artifacts` (SHA-256, `sha256sum`) the `.qnt` file and a normalized **result summary**, not the raw log (`verify.summary`): the Quint and Apalache versions, the exact command line (bounds, `--invariant`) and the verdict without durations (`quint verify` prints a time). A `quint verify` run is exhaustive up to its bounds and reproducible; `quint run` is random sampling: fix `--seed`, `--max-samples` and `--max-steps` and put them in the summary, or keep it as unhashed `analysis`; editing any of them makes `--check-artifacts` fail and name the record. Keep the Quint and Apalache versions and assumptions with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- Keep the raw log out of the repository or unhashed (git-ignored): it carries pids, timings and absolute local paths and changes on every run, so its hash proves only that the file was not edited, not that it came from the hashed model.
- Rerun in CI when the configuration is fast: regenerate the summary from the hashed model and fail on any difference from the committed one (`diff`), so a model edit cannot pass by updating hashes and a mismatch is a real failure, not noise. Slow configurations stay manual and pinned by hash only: record them as such (`verifier`, `timestamp`) and rerun after any change.
- Contract changes invalidate the evidence; rerun after `--impact <capability>`.
- The validator does not run Quint: run it in CI and produce the evidence there.

## Prompt

"Write a Quint module for `<capability>` from its contract; one named `val` per invariant, one action per operation and a nondeterministic `step`; give the `quint run` and `quint verify` commands with bounds and list assumptions."
