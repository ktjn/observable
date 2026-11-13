# Modernization and TDD Feedback

Date: 2026-11-10.
Source: design review of a modernization track for onboarded brownfield systems ([`MODERNIZATION.md`](../../MODERNIZATION.md)), with DDD and TDD as implementation methodology.

## Findings

| ID | Observed fact | Gap | Disposition |
|---|---|---|---|
| MDN-FB-01 | Test-first unit tests are many and fine-grained; [`spec.md` § 8](../../spec.md#8-verification) requires every test to bind to exactly one requirement version (`--check-report`). | No stated home for unit tests below a component requirement. | Resolved by guidance: unit tests bind to the component sub-requirement (`refines`) whose criterion they elaborate ([FEATURE.md](../../FEATURE.md#decompose-and-hand-off), [constitution § Traceability](../../CONSTITUTION.md#traceability)). A test that elaborates no criterion means a missing criterion or sub-requirement. No model change. |
| MDN-FB-02 | `interface.idempotency_strategy` and the intake storage rules (OBS-FB-01 to 03: schema namespaces, `ReplacingMergeTree`) name mechanisms in contracts and methodology. | Mechanism leakage: a next generation cannot question a mechanism the contract requires. | Open. Guidance in MODERNIZATION.md: treat a technology-naming clause as hidden coupling; prefer the observable property (no duplicate visible rows) over the mechanism. |
| MDN-FB-03 | `quality_attributes` is parsed only; brownfield waives performance, scalability and resilience until release. | Mechanism choices cannot be justified or rejected without measured forces. | Guidance: MODERNIZATION.md prerequisites require characterized quality requirements. No model change. |
| MDN-FB-04 | Requirements lack rationale/source (see [BENCHMARK.md](../BENCHMARK.md) gaps); decisions live only in gate records. | No link from a mechanism decision to the spec. | Template [`MODERNIZATION-DECISION.md`](../../templates/MODERNIZATION-DECISION.md); `Refinement.decision` references it in a baseline import. No model change. |
| MDN-FB-05 | Baseline imports check requirements, risks, views, experiences and state sets, not capabilities or components. | A next-generation spec re-declares capabilities. | Use `--compat <as-is>` for capabilities and data models; components are re-derived by design. No model change. |
