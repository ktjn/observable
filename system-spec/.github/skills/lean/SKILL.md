---
name: lean
description: Prove a capability's invariants with Lean 4 (a machine-checked, unbounded proof about a model) and record the result as metamodel Evidence. Use when the constitution's formal verification assessment flags a component (financial transactions, safety-critical logic, state machines, algorithms, protocols) and a bounded model check is not enough, or the property quantifies over unbounded values, sizes or steps.
---

# Lean Verification

Rules: [constitution § Formal Verification](../../../CONSTITUTION.md#formal-verification). Format: [`spec.md`](../../../spec.md) §§ 4, 8. Complements the bounded checkers [`tla`](../tla/SKILL.md) and [`quint`](../quint/SKILL.md) (exhaustive up to bounds, no proof effort) and [`alloy`](../alloy/SKILL.md) (structure, relations): use Lean when the property must hold for all inputs and steps. Check commands against `lake --version` for your toolchain.

## Inputs

- Target capability: `preconditions`, `postconditions`, `invariants`, `failure_modes`.
- Requirement(s) to verify, with `criteria`.
- Consistency boundary of the owning component.

## Steps

1. **Project**: `lake new <name>` (or add a library to an existing `lakefile`); commit `lean-toolchain` and `lake-manifest.json` so the proof rebuilds with the same versions. Add Mathlib only if needed.
2. **Model**: one file per capability, e.g. `Inventory.lean`.
   - `structure State`: only state named by the contract.
   - One function or relation per operation; guard with `preconditions`; model `failure_modes` as explicit outcomes (`Option`, `Except`, a result type).
   - An inductive `Step : State → State → Prop` (or a `Reachable` closure) models any interleaving of operations at the component's consistency boundary.
3. **Theorems**: one per contract clause, named after it.
   - `invariants` → `theorem stockNeverNegative : Reachable s → 0 ≤ s.stock` by induction on `Reachable` (an inductive invariant: `init` establishes it, every `Step` preserves it).
   - `postconditions` → `theorem reserve_post : reserve s q = some s' → s'.stock = s.stock - q`.
   - Liveness only when a requirement demands it; state fairness as an explicit hypothesis.
4. **Assumptions**: every `axiom`, `variable` hypothesis and modelled-away behaviour is an assumption; list it, never hide it in the model.
5. **Check**, in this order:
   - `lake build`: the kernel checks every proof; any `sorry` is a warning, treat it as failure.
   - `#print axioms stockNeverNegative`: allowed are the standard `propext`, `Classical.choice`, `Quot.sound`; `sorryAx` or a project `axiom` means the proof is incomplete or conditional.
   - Optional: `#eval` or `example := by decide` on concrete cases as a sanity check of the model (not proof).
6. **Review**: confirm the model corresponds to the contract and implementation; a proof about the model is not a proof about the code. On a failed proof, fix spec or design, never weaken the theorem silently.
7. **Correspondence**: a proof about the model is not a proof about the code ([`VERIFIED-DEVELOPMENT.md` §5](../../../VERIFIED-DEVELOPMENT.md)). Tie them with at least one of:
   - differential tests of the implementation against the Lean functions (`#eval` or a compiled executable);
   - a refinement theorem from an abstraction function over implementation state, when the implementation is itself modelled in Lean;
   - a verified extraction toolchain, if the project uses one.
   Record each as `test` (or `analysis`) evidence bound to the same requirement (`verifies`, [`spec.md` § 8](../../../spec.md#8-verification)). Without one, the requirement is verified by the model only: `--evidence-report` lists it with `model_only: true`.

## Evidence

```yaml
evidence:
  - id: ev-reserve-lean
    requirement_id: req-reserve
    requirement_version: "1.0"
    method: proof
    passed: true
    provenance: lean::Inventory.lean
    artifacts: { Inventory.lean: <sha256>, lean-toolchain: <sha256>, lake-manifest.json: <sha256>, axioms.summary: <sha256> }
```

- A complete `lake build` with no `sorry` and only standard axioms is `method: proof`; no `bounds` are needed (`bounds` are for `model-check`, `EV-01`).
- It proves the theorem about the model, under the listed assumptions; the model-to-code gap stays open unless step 7 closes it, so state what was proven and about what. A correspondence check is `test` evidence, not part of the `proof` record.
- Hash in `artifacts` (SHA-256, `sha256sum`) every `.lean` file the theorem depends on, `lean-toolchain`, `lake-manifest.json` and a normalized **result summary**, not the raw log (`axioms.summary`): the `lake build` exit status and the sorted `#print axioms` lines (`'stockNeverNegative' depends on axioms: [propext]`); build progress, timings and cache lines vary, the axiom list of a proof does not; editing any of them makes `--check-artifacts` fail and name the record. Keep the assumptions and assumptions with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- Keep the raw log out of the repository or unhashed (git-ignored): it carries pids, timings and absolute local paths and changes on every run, so its hash proves only that the file was not edited, not that it came from the hashed model.
- Rerun in CI when the configuration is fast: regenerate the summary from the hashed model and fail on any difference from the committed one (`diff`), so a model edit cannot pass by updating hashes and a mismatch is a real failure, not noise. Slow configurations stay manual and pinned by hash only: record them as such (`verifier`, `timestamp`) and rerun after any change.
- Contract changes invalidate the evidence; rerun after `--impact <capability>`.
- The validator does not run Lean: run `lake build` in CI and produce the evidence there.

## Prompt

"Write a Lean 4 model of `<capability>` from its contract: a `State` structure, one function per operation, an inductive `Step` and one named theorem per invariant/postcondition proved by induction; give the `lake build` and `#print axioms` commands and list assumptions."
