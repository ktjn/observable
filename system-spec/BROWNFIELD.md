# Brownfield: Onboarding Existing Systems

Incremental sequencing for existing systems. Artifact rules: [constitution](CONSTITUTION.md). Load this document together with the constitution only.

Overrides the [greenfield](GREENFIELD.md) ordering: existing code is the discovery input, so architecture may be described before capabilities, and implementation precedes verification.

## Adoption Levels

Each level targets a `--stage` ([`spec.md` § 11](spec.md#11-metamodel-validation)); known violations are allowed below `release` as risk waivers ([`spec.md` § 7](spec.md#7-risks)). Level 0 advances directly to Level 1 (`architecture`), skipping standalone `discovery` because existing code supplies architecture and contracts together. Validate with `metamodel.py <spec> --stage <stage>`.

| Level | Goal | Stage |
|---|---|---|
| 0 — Inventory | `system`, `external_sources`, deployable units, coarse module boundaries (`status: draft`); identity and syntax only | `inventory` |
| 1 — Dependency & data mapping | Observed `provides`/`requires`, data ownership, classification, lineage (`status: specified`); layer/cycle/ownership/budget violations recorded as waivers | `architecture` |
| 2 — Behavioral pinning | As-is contracts and characterization tests (`Evidence`); missing verification categories covered by waivers; external contracts frozen before refactoring | `verification` |
| 3 — Full conformance | All waivers remediated; layer and context budget rules enforced without exception; all 10 verification categories satisfied (`status: verified`) | `release` |

For each adoption level, use the [shared decision record](CONSTITUTION.md#gate-decision-record): review observed artifacts against the level's criteria, record discrepancies and decide whether to pass, defer via an allowed waiver or rework.

| Level | Bring to review | Decision required before proceeding |
|---|---|---|
| 0 — Inventory | Entrypoints, jobs, external sources, deployable units and candidate component boundaries | All active runtime units and external dependencies are registered; unknown boundaries are flagged for discovery |
| 1 — Dependency & data mapping | Observed capability wiring, data schemas/owners/lineage, and discovered architecture violations with proposed waivers | Observed dependencies and data ownership match the running code; every violation is either remediated or deferred under a scoped, non-expired waiver |
| 2 — Behavioral pinning | As-is contracts, characterization test results for external/cross-component behavior, compatibility baseline and remaining coverage waivers | External and cross-component behavior is pinned by passing characterization evidence; unverified categories have bounded waivers expiring by release |
| 3 — Full conformance | Remediated architecture, compatibility results (`--compat`), requirement-linked evidence across all ten categories and risk dispositions | All waivers are closed, contracts and invariants hold without exception, and release evidence is current and reviewed |

If a level review fails (for example, an unrecorded dependency, a characterization test that contradicts the drafted contract, or an expired waiver), keep the level open: correct the specification to match observed behavior or complete the remediation before advancing. Do not hide discrepancies by inventing idealized contracts during discovery.

Worked progression from Level 0 to Level 3: [`examples/brownfield/`](examples/brownfield/README.md).

Real onboarding feedback: [Modelable](archive/feedback/modelable-onboarding.md) records bounded adoption, existing authority/tool mapping, evidence revision binding and unresolved model gaps; it is not a full-conformance example. [Observable](archive/feedback/observable-onboarding.md) records complete brownfield onboarding across all 6 milestones to Level 3 release conformance, covering multi-schema database isolation, schema compatibility matrices, and automated evidence provenance.

## Reverse Discovery

Start with the [intake and gate template](templates/BROWNFIELD-INTAKE.md): record the repository root, exact source revision, adoption scope, existing authorities, product/validator tool map, validator/schema pin and native gates. Use separate whole-product and slice decisions; a slice pass does not advance excluded components. Record external/deferred/unknown boundaries explicitly; an unresolved required provider keeps architecture open. Imported schemas and capability catalogs are discovery inputs, not acceptance evidence.

For local multi-step operations, record transitions, actual Python/Rust/etc. interfaces, confirmation/freshness checks, durable writes and recovery. Distinguish failed-before-commit from committed-with-cleanup-error. Keep transaction ownership intact when choosing context budgets. The template supplies the protocol review record without inventing a new transport or claiming executable state-machine verification.

1. **Boundaries & entrypoints**: APIs, jobs, events, external sources → `system`, capability stubs, `external_sources`.
2. **Data & state**: schemas, migrations, ORM/DTO models (use [`modelable`](.github/skills/modelable/SKILL.md) `import` where helpful) → `data_models`, owners, classification, lineage.
3. **Components & dependencies**: cluster modules by transaction/consistency boundaries → `components`, layers, wiring.
4. **Invariants & failures**: domain guards, error handlers, existing tests → capability contracts, `Requirement.criteria`, initial `Evidence` (use [`tla`](.github/skills/tla/SKILL.md) / [`alloy`](.github/skills/alloy/SKILL.md) for concurrency or relational invariants).
5. **Gaps & risks**: missing verification categories, unverified mitigations, architecture violations → `risks` and `requirements` backlog.

Rules:

- Record observed behavior, not intended behavior; never invent contracts or fields.
- Separate **as-is discrepancies** (bugs, coupling, undocumented side effects observed in code) from **target changes** (remediation or modernization requirements).
- Validate with `metamodel.py` after each step and review the underlying code and test evidence before recording a level decision.
- Unknown/provisional classification uses `classification: unknown` plus an active `classification_review` requirement and conservative handling policy. Resolve it before architecture; do not allow an implicit public classification.
- Bind implementation evidence to contract versions and source revisions with `subject_bindings`/`subject_version`; preserve file hashes, execution conditions and explicit `skipped` checks. Run optional `--check-artifacts --root <repository>` for local source/schema/evidence integrity, `--check-drift --root <repository>` for Python import and JSON Schema field drift, then retain native semantic and acceptance gates.
- Evidence paths are relative to that repository root, even when the specification lives in a nested directory. Use one provenance file with an optional `::selector`; declare additional logs and environment records separately in `artifacts`. Preserve the original schema, capture and decisions when migrating validator versions.
- Attribute skips to the requirements they leave unverified. Separate passing records may cover genuinely independent, executed checks; a suite's passing count does not establish a skipped criterion. Check release diagnostics for binding errors without treating their absence as release acceptance.
- When components share relational storage or streaming buses, enforce dedicated schema namespaces, eliminate cross-schema foreign keys, and back idempotency guarantees with formal models (`TelemetryDelivery.tla`) and concrete deduplication mechanisms.
- Require transitional boundary allowlists to monotonically ratchet downward to zero before Level 3 acceptance.

## Waivers

- Known violations (`layer-violation`, `dependency-cycle`, `shared-data-ownership`, `context-budget`, `verification-category`) are linked to a `Risk` (`waives`) with `treatment: mitigation` or `acceptance`.
- Each waiver requires a remediation requirement in `mitigated_by` and an `expires` bound (ISO date `YYYY-MM-DD` or target stage).
- Expired waivers fail validation at every stage; no waivers are allowed at `--stage release` (Level 3).

## Characterization, Migration and Acceptance Loop

1. **Pin baseline (Levels 1–2)**: capture observed contracts and run characterization tests before changing internal structure. Record any deferred violations or coverage gaps as scoped waivers with remediation requirements.
2. **Scope one migration step**: run `--impact <id>` against the pinned baseline to identify affected components, data models, requirements and evidence. Prepare a bounded [implementation handoff](CONSTITUTION.md#implementation-handoff) (`--context <component-id>`) containing the pinned contract, target remediation requirement, allowed boundary changes and characterization suite.
3. **Refactor or extract**: implement the strangler or boundary change within the handoff scope. If hidden coupling or an undocumented failure mode surfaces, stop and reopen the Level 1 or Level 2 review to update the observed baseline first.
4. **Verify compatibility and remediation**: check `--compat <pinned-baseline>` for capability and data contracts, rerun invalidated characterization and remediation tests, and update evidence revisions. If a waiver expires or a check fails, recover by completing the remediation or rolling back the step before proceeding.
5. **Close waivers and accept (Level 3)**: remove remediated waivers once their mitigating requirements have passing, current evidence. Complete all ten verification categories and record the release decision ([`CONSTITUTION.md` § Acceptance](CONSTITUTION.md#acceptance)).

## Prompts

Ask for observed evidence and explicit discrepancy decisions. When serializing the reference format, validate against [`reference-implementation/schema.json`](reference-implementation/schema.json) (`metamodel.py --schema`) and [`spec.md` § 11](spec.md#11-metamodel-validation) invariants; prose findings and review records remain part of the process. Per step:

- **1** (`REF-01`, `DATA-01`): "List entrypoints of `<repo>` as draft `capabilities`, `components` and `external_sources` (`REF-01`, `DATA-01`); record observed behavior only."
- **2** (`DATA-01`–`DATA-04`): "Extract `data_models` from `<schemas>` with `owner`, `classification` and `derived_from` (`DATA-01`, `DATA-02`, `DATA-03`, `DATA-04`); do not invent fields."
- **3** (`STAT-02`, `COMP-01`–`COMP-07`): "Cluster `<modules>` into `components` by consistency boundary; list observed `provides`/`requires` and violations (`STAT-02`, `COMP-01`, `COMP-02`, `COMP-03`, `COMP-04`, `COMP-05`, `COMP-06`, `COMP-07`)."
- **4** (`STAT-01`, `CAP-01`–`CAP-04`): "Derive `preconditions`, `postconditions`, `invariants`, `failure_modes` and `interface` for `<capability>` from guards, handlers and tests (`STAT-01`, `CAP-01`, `CAP-02`, `CAP-03`, `CAP-04`)."
- **5** (`REQ-01`, `REQ-07`, `RISK-01`–`RISK-08`): "List missing verification categories and architecture violations as `risks` (`waives`, `mitigated_by`, `expires`) and `requirements` (`REQ-01`, `REQ-07`, `RISK-01`, `RISK-02`, `RISK-03`, `RISK-04`, `RISK-08`)."
- **Level & waiver review** (`EV-04`, `RISK-01`, `RISK-02`, `RISK-08`): "Compare these spec revisions against the observed code/tests and this level's criteria (`EV-04`, `RISK-01`, `RISK-02`, `RISK-08`). Separate observed behavior from intended fixes, verify waiver scope/expiry and record pass, defer or rework."
- **Migration handoff** (`COMPAT-01`–`COMPAT-09`): "Refactor `<component>` within this bounded `--context` handoff while preserving the pinned contracts (`COMPAT-01`, `COMPAT-02`, `COMPAT-03`, `COMPAT-04`, `COMPAT-05`, `COMPAT-06`, `COMPAT-07`, `COMPAT-08`, `COMPAT-09`) and characterization tests. Report affected revisions, compatibility findings and any newly discovered facts, coupling or model gaps ([self-improvement](CONSTITUTION.md#self-improvement-and-model-feedback))."
