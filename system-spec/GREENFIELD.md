# Greenfield: Creating New Systems

Contract-first sequencing for new systems. Artifact rules: [constitution](CONSTITUTION.md). Load this document together with the constitution only.

## Phases

| # | Phase | Output | Gate |
|---|---|---|---|
| 1 | Capability discovery | Capabilities with contracts and failure modes | 1 |
| 2 | Invariant discovery | Invariants per capability | 1 |
| 3 | Component identification | Components derived from invariants and consistency boundaries | 2 |
| 4 | Component specification | Component definitions; no implementation details | 2 |
| 5 | Data contracts | Data models with owners | 2 |
| 6 | API contracts | API contracts per capability | 2 |
| 7 | Verification specification | Requirements covering all 10 categories | 3 |
| 8 | Formal verification assessment | Formal models where indicated | 3 |
| 9 | Layering | Layers assigned; rules hold | 2 |
| 10 | Context budget | Oversized components split | 2 |
| 11 | Traceability | All artifacts linked | 4 |
| 12 | Acceptance | Passing, current evidence for all mandatory requirements | 4 |

Rules:

- Do not discuss technical architecture before capabilities are identified.
- Phase numbers identify work products; execution follows gate order: **1–2 → 3–6 and 9–10 → 7–8 → implementation → 11–12**.
- Begin implementation only after the discovery, architecture and verification gates pass for the change's scope.

## Gates (`--stage`, see [`spec.md` § 11](spec.md#11-metamodel-validation))

1. **Discovery** (`--stage discovery`): Phases 1–2; capabilities `specified` with non-empty contract clauses.
2. **Architecture** (`--stage architecture`): Phases 3–6, 9–10; components `specified`, dependencies resolved, layer rules, data models, interface contracts and context budgets satisfied.
3. **Verification** (`--stage verification`): Phases 7–8; requirements `specified` across all 10 categories; formal models checked with [`tla`](.github/skills/tla/SKILL.md) or [`alloy`](.github/skills/alloy/SKILL.md) where indicated.
4. **Release** (`--stage release`): Phases 11–12; all active entities `verified`, passing current `Evidence` for all mandatory and mitigating requirements, all risks disposed, zero waivers.

For each gate, use the [shared decision record](CONSTITUTION.md#gate-decision-record): review the artifacts, record findings and decide whether to proceed or rework.

| Gate | Bring to review | Decision required before proceeding |
|---|---|---|
| Discovery | User/business outcome, consumers, capability contracts, invariants and failure cases | The scope and observable success/failure behavior are understood; material questions are resolved |
| Architecture | Component boundaries, dependency/layer map, ownership, data/API contracts and context budget | The composition can preserve the invariants; every dependency has an explicit contract and owner |
| Verification | Requirement-to-criterion mapping across all ten categories, planned tests/inspections, formal assessment and assumptions | Each obligation has a meaningful way to establish pass/fail; applicable formal checks are reviewed before implementation |
| Release | Requirement-linked execution results for current revisions, traceability, consumer compatibility/migration results and risk dispositions | All mandatory and mitigating obligations are satisfied, unresolved failures are addressed and no waiver remains |

If a gate fails, return to the earliest affected phase; do not advance by relabeling an incomplete artifact. Validate each gate with `metamodel.py <spec> --stage <gate>` and pass only the target component's slice (`--context <component-id>`) to the coding agent. At implementation (AI Execution Rule step 9), derive `.mdl` and typed data artifacts with [`modelable`](.github/skills/modelable/SKILL.md).

## Implementation and Acceptance Loop

1. Select one capability change whose first three gates have passed. Prepare the [implementation handoff](CONSTITUTION.md#implementation-handoff).
2. Make verification executable before implementing the intended behavior; confirm a meaningful failing case where behavior is missing. Reused passing checks still need reviewed coverage of the change.
3. Implement within the accepted component boundary. Return to the affected gate if a contract, ownership or dependency must change.
4. Run the specified verification and inspect results. Record failures as well as successes with requirement IDs and applicable revisions.
5. Review traceability, compatibility, evidence freshness and risk treatment at release. Close the scope only after recording the acceptance decision; carry unresolved work explicitly.

Worked example traced through Phases 1–12: [`examples/greenfield/`](examples/greenfield/README.md).

## AI Execution Rule

When asked to design or implement any feature, produce artifacts in this order:

```text
1. Capability
2. Invariants
3. Component definition
4. Contracts
5. Data model
6. Verification specification
7. Formal model (if applicable)
8. Architecture impact analysis (--impact)
9. Implementation
10. Verification results
```

- For each step, produce the artifact or cite the reviewed artifact being reused with an impact justification.
- Never generate implementation before verification has been specified.
- Treat architecture as provable constraints, not a collection of code files.

## Prompts

Ask for the artifact and the decisions it supports. When serializing the reference format, validate against [`reference-implementation/schema.json`](reference-implementation/schema.json) (`metamodel.py --schema`) and [`spec.md` § 11](spec.md#11-metamodel-validation) invariants; prose decisions and review evidence remain part of the process. Per phase:

- **1–2** (`STAT-01`, `CAP-01`): "List capabilities of `<domain>` (`status: specified`) with non-empty `preconditions` or `postconditions`, `invariants` and `failure_modes` (`STAT-01`, `CAP-01`); no architecture."
- **3–4** (`STAT-02`, `COMP-01`–`COMP-07`): "Derive components (`status: specified`) from these invariants and consistency boundaries; assign `layer`, `provides`, `requires` and `owns_data` within `context_budget` (`STAT-02`, `COMP-01`, `COMP-02`, `COMP-03`, `COMP-04`, `COMP-05`, `COMP-06`, `COMP-07`)."
- **5–6** (`DATA-01`–`DATA-04`, `CAP-02`–`CAP-03`): "Define `data_models` and capability `interface` contracts for `<capability>` mapping every failure mode in `errors` (`DATA-01`, `DATA-02`, `DATA-03`, `DATA-04`, `CAP-02`, `CAP-03`); do not invent fields."
- **7–8** (`STAT-03`, `CAP-04`, `REQ-01`–`REQ-03`, `REQ-07`, `EV-01`): "Write `requirements` (`status: specified`, `category`, matching `kind`) for all 10 verification categories of `<capability>` (`STAT-03`, `CAP-04`, `REQ-01`, `REQ-02`, `REQ-03`, `REQ-07`); flag formal verification candidates (`EV-01`)."
- **Gate review** (`STAT-04`, `EV-04`, `REQ-04`–`REQ-06`, `RISK-01`–`RISK-08`): "Review these artifact revisions against this gate's criteria and invariants (`STAT-04`, `EV-04`, `REQ-04`, `REQ-05`, `REQ-06`, `RISK-01`, `RISK-02`, `RISK-08`). Identify contradictions and missing evidence; record pass or rework with the next action. Do not infer acceptance from validator output."
- **Implementation** (`COMPAT-01`–`COMPAT-09`): "Implement this bounded `--context` handoff against the accepted contracts and tests (`COMPAT-01`, `COMPAT-02`, `COMPAT-03`, `COMPAT-04`, `COMPAT-05`, `COMPAT-06`, `COMPAT-07`, `COMPAT-08`, `COMPAT-09`). Return requirement-linked results, affected revisions and any newly discovered facts or model gaps ([self-improvement](CONSTITUTION.md#self-improvement-and-model-feedback)); surface contract changes before implementing dependent behavior."
