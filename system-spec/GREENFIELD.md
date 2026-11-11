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
3. **Verification** (`--stage verification`): Phases 7–8; requirements `specified` across all 10 categories; formal models checked with [`tla`](.github/skills/tla/SKILL.md), [`quint`](.github/skills/quint/SKILL.md) or [`alloy`](.github/skills/alloy/SKILL.md) where indicated.
4. **Release** (`--stage release`): Phases 11–12; all active entities `verified`, passing current `Evidence` for all mandatory and mitigating requirements, all risks disposed, zero waivers.

For each gate, use the [shared decision record](CONSTITUTION.md#gate-decision-record): review the artifacts, record findings and decide whether to proceed or rework.

| Gate | Bring to review | Decision required before proceeding |
|---|---|---|
| Discovery | User/business outcome, consumers, capability contracts, invariants and failure cases | The scope and observable success/failure behavior are understood; material questions are resolved |
| Architecture | Component boundaries, dependency/layer map, ownership, data/API contracts and context budget | The composition can preserve the invariants; every dependency has an explicit contract and owner |
| Verification | Requirement-to-criterion mapping across all ten categories, planned tests/inspections, formal assessment and assumptions | Each obligation has a meaningful way to establish pass/fail; applicable formal checks are reviewed before implementation |
| Release | Requirement-linked execution results for current revisions, traceability, consumer compatibility/migration results and risk dispositions | All mandatory and mitigating obligations are satisfied, unresolved failures are addressed and no waiver remains |

If a gate fails, return to the earliest affected phase; do not advance by relabeling an incomplete artifact. Validate each gate with `metamodel.py <spec> --stage <gate>` and pass only the target component's slice (`--context <component-id>`) to the coding agent. At implementation (AI Execution Rule step 9), derive `.mdl` and typed data artifacts with [`modelable`](.github/skills/modelable/SKILL.md).

## Handoff to specification-controlled development

Greenfield work begins with intended outcomes rather than preexisting code. Discovery establishes observable behavior, invariants and failures; architecture identifies logical boundaries, layers and domains independently of physical source layout. Verification obligations are specified before implementation, including assumptions and applicable failure scenarios.

At the verification gate, the scope's reviewed specification becomes the accepted behavioral baseline for implementation. Record any unknown or deliberately unconstrained behavior; do not substitute implementation details for missing contracts. Implementation agents may choose language, algorithm and physical decomposition within the project's adopted constraints.

### Planned components

Architecture decides the domain of a component before its code exists. When the architecture gate runs before a component can be declared `specified`, declare its domain and `uses` in `system.domains` and name the planned component in the domain description, so `DOM-03` sees the would-be edges. Check that no planned dependency closes a cycle with an existing one, and record which direction is forbidden.

### Adopting an upstream specification

When a business or product specification written elsewhere becomes the requirement baseline of an implementation specification:

1. Vendor the upstream files unchanged at a pinned commit, with a digest manifest (`sha256  path` per file) and a source note. A newer upstream version is a new sibling directory.
2. Derive the implementation fragments from it. Declare field policies for systematic changes (retargeting to the implementation's capabilities, version reset, ID prefixes, relocated assets).
3. Declare every other change or removal of baseline content with a reason and a decision reference. Additions (new interactions, scenarios, list items) need no declaration because they add obligations.
4. Check in the project gate: manifest digests, no undeclared change or removal, no stale declaration.
5. Do not retire the upstream repository before the snapshot is merged.

`system.imports` with `mode: baseline` records steps 1–4 in the spec: `artifacts` digests, `allow_fields` and `id_prefix` for systematic changes, and `refinements` (`path`, `reason`, `decision`); `--check-imports --root` performs the check.

After the first iteration is accepted, use [FEATURE.md](FEATURE.md) for behavioral evolution and [REPLACEMENT.md](REPLACEMENT.md) for an independently constructed alternative implementation. New evidence is required for each new implementation revision.

## First iteration

Moves a scope that passed the verification gate from specification to its first running, evidenced build. Nothing here relaxes the gates; the first iteration is a bounded slice of the accepted baseline.

Entry: `--stage verification` is `VALID` for the scope and the gate decision is recorded. Unknown or unconstrained behavior is listed, not guessed.

1. **Freeze the baseline.** Save the reviewed spec as the old spec (`old.yaml`, or the VCS revision) for `--compat`. From here changes follow [FEATURE.md](FEATURE.md); implementation never edits contracts to fit code.
2. **Choose the slice.** Give requirements a `priority` and set `system.release_scope` (for example `[P0]`) to the smallest set that exercises one consumer-to-provider path through every layer (walking skeleton). Everything else is reported deferred, never passed. Every in-scope requirement keeps its obligations.
3. **Plan.** `--plan <capability-id>` for each in-scope capability gives provider-first waves, slices and evidence to produce. Components in a wave run in parallel.
4. **Bind.** Per component, add `source_paths`, `test_paths`, `fakes` and `schema_path` as the code appears. These are first-iteration outputs; earlier gates do not require them. Review bindings with the component's owner.
5. **Build per handoff.** Prepare each [implementation handoff](CONSTITUTION.md#implementation-handoff) from `--context <component-id>`. Tests exist and fail before behavior is implemented. Check with `--check-scope` and `--check-return`.
6. **Record evidence.** Tests carry `verifies: <requirement>@<version>`; derive records with `--emit-evidence` and compare with `--check-report`. Failures are recorded too. Set `evidence_policy.require_criteria` when partial coverage is not acceptable.
7. **Advance status.** Set entities to `verified` only with passing, current evidence. `--evidence-report` shows what is still uncovered.
8. **Check the iteration.** `--stage release --release-scope <scope>` with `--check-artifacts --check-drift --root .`. Record the [gate decision](CONSTITUTION.md#gate-decision-record): scope, deferred list, residual risks, waivers (none at `release`).
9. **Route discoveries.** A fact that contradicts the baseline reopens the earliest affected gate. A model or validator gap goes to [feedback](CONSTITUTION.md#self-improvement-and-model-feedback). Do not patch around either in code.
10. **Iterate.** Widen `release_scope` (or add a feature) and repeat from step 3. When no priority is deferred, the system is fully released.

Trade-offs:

- Narrow slice: early evidence and fast contract feedback; deferred scope stays unproven and may hide cross-slice coupling.
- Wide slice: fewer iterations; late discovery, larger rework when a contract changes.
- Binding at step 4 rather than at architecture avoids inventing paths, but delays `--check-drift` until code exists.

NOTE: A passing release-scope run accepts the slice only. It says nothing about deferred requirements.

## Component isolation

When [component isolation](COMPONENT-ISOLATION.md) is adopted, derive required/provided capability seams at architecture design time; specify a standalone invocation harness and a programmable provider-owned mock for each provided capability. Build consumers against those published mocks without waiting for real providers. At verification, check both standalone execution and mock-versus-provider contract conformance; integration verification remains separate.

## Implementation and Acceptance Loop

1. Select one capability change whose first three gates have passed. Prepare the [implementation handoff](CONSTITUTION.md#implementation-handoff).
2. Make verification executable before implementing the intended behavior; confirm a meaningful failing case where behavior is missing. Reused passing checks still need reviewed coverage of the change. Inner-loop unit tests bind to the component sub-requirement they elaborate ([constitution § Traceability](CONSTITUTION.md#traceability)).
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
- **3–4** (`STAT-02`, `COMP-01`–`COMP-07`, `DOM-01`–`DOM-03`): "Derive components (`status: specified`) from these invariants and consistency boundaries; assign `layer`, `domain` (cross-domain dependencies in `uses`), `provides`, `requires` and `owns_data` within `context_budget` (`STAT-02`, `COMP-01`, `COMP-02`, `COMP-03`, `COMP-04`, `COMP-05`, `COMP-06`, `COMP-07`, `DOM-01`, `DOM-02`, `DOM-03`)."
- **5–6** (`DATA-01`–`DATA-04`, `CAP-02`–`CAP-03`): "Define `data_models` and capability `interface` contracts for `<capability>` mapping every failure mode in `errors` (`DATA-01`, `DATA-02`, `DATA-03`, `DATA-04`, `CAP-02`, `CAP-03`); do not invent fields."
- **7–8** (`STAT-03`, `CAP-04`, `REQ-01`–`REQ-03`, `REQ-07`, `EV-01`): "Write `requirements` (`status: specified`, `category`, matching `kind`) for all 10 verification categories of `<capability>` (`STAT-03`, `CAP-04`, `REQ-01`, `REQ-02`, `REQ-03`, `REQ-07`); flag formal verification candidates (`EV-01`)."
- **Gate review** (`STAT-04`, `EV-04`, `REQ-04`–`REQ-06`, `RISK-01`–`RISK-08`): "Review these artifact revisions against this gate's criteria and invariants (`STAT-04`, `EV-04`, `REQ-04`, `REQ-05`, `REQ-06`, `RISK-01`, `RISK-02`, `RISK-08`). Identify contradictions and missing evidence; record pass or rework with the next action. Do not infer acceptance from validator output."
- **Implementation** (`COMPAT-01`–`COMPAT-09`): "Implement this bounded `--context` handoff against the accepted contracts and tests (`COMPAT-01`, `COMPAT-02`, `COMPAT-03`, `COMPAT-04`, `COMPAT-05`, `COMPAT-06`, `COMPAT-07`, `COMPAT-08`, `COMPAT-09`). Return requirement-linked results, affected revisions and any newly discovered facts or model gaps ([self-improvement](CONSTITUTION.md#self-improvement-and-model-feedback)); surface contract changes before implementing dependent behavior."
