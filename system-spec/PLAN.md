# Roadmap: Greenfield & Brownfield System Specification

## 1. Goal & Scope

Lifecycle model, metamodel, and reference tooling supporting:

1. **Creating new systems (Greenfield)** — contract-first, phase-gated design, verification, and bounded implementation handoffs.
2. **Onboarding existing systems (Brownfield)** — incremental reverse-discovery, architectural debt waivers, characterization pinning, and safe strangler migration.

### 1.1 Document Structure
- [`CONSTITUTION.md`](CONSTITUTION.md): shared fundamentals, artifact rules, gate decision records, implementation handoffs, and change/reverification loop.
- [`GREENFIELD.md`](GREENFIELD.md): phase sequence, gates, implementation loop, AI Execution Rule, and prompts.
- [`BROWNFIELD.md`](BROWNFIELD.md): adoption levels, reverse discovery, waivers, characterization/migration loop, and prompts.
- [`spec.md`](spec.md): normative metamodel, stage invariant table (§ 11), and current reference format.
- [`WIRING.md`](WIRING.md) and [`templates/BROWNFIELD-INTAKE.md`](templates/BROWNFIELD-INTAKE.md): toolkit adoption guide and brownfield intake/gate template.
- [`.github/skills/<name>/SKILL.md`](.github/skills/): technique guides ([`tla`](.github/skills/tla/SKILL.md), [`alloy`](.github/skills/alloy/SKILL.md), [`modelable`](.github/skills/modelable/SKILL.md)).
- [`examples/`](examples/): end-to-end [greenfield](examples/greenfield/README.md) (incl. [`Inventory.tla`](examples/greenfield/Inventory.tla) and [`Inventory.als`](examples/greenfield/Inventory.als)) and [brownfield](examples/brownfield/README.md) walkthroughs.
- [`reference-implementation/`](reference-implementation/README.md): validator (`metamodel.py`), JSON Schema (`schema.json`), Alloy metamodel (`metamodel.als`), and invariant test corpus (`corpus.yaml`).
- [`archive/`](archive/README.md): historical [review log](archive/REVIEW.md) and [standards benchmark](archive/BENCHMARK.md).
- AI context per task: constitution + one track document + applicable skill + `--context <component-id>` slice.

---

## 2. Delivered Architecture & Metamodel (`spec.md`)

- **Lifecycle, Adoption & Stages** ([`spec.md` §§ 10–11](spec.md#10-lifecycle-and-evolution))
  - Entity `status` (`capabilities`, `components`, `requirements`, `system`): `draft` → `specified` → `verified` → `deprecated`.
  - Validation `--stage` (`inventory` → `discovery` → `architecture` → `verification` → `release`): 55 stage invariants + 9 `--compat` invariants (`REF`, `DATA`, `EV`, `RISK`, `STAT`, `CAP`, `COMP`, `REQ`, `COMPAT`), drift-tested against [`README.md`](README.md) and [`spec.md`](spec.md).
  - Brownfield adoption levels map to stages: Level 0 (`inventory`) → Level 1 (`architecture`, skipping standalone `discovery` because `architecture` enforces `CAP-01`) → Level 2 (`verification`) → Level 3 (`release`).
- **Explicit Interface & Data Contracts** ([`spec.md` §§ 4–5](spec.md#4-capabilities))
  - Cross-component capabilities declare `Capability.interface` (`operation`, `mode`, `request`, `response`, `idempotent`, `auth`, `errors` matching `failure_modes`).
  - Data models enforce single ownership (`DATA-02`, `DATA-03`), lineage classification propagation (`DATA-04`), and verified declassification (`declassified_by`).
- **Context Budgets & Artifact Bindings** ([`spec.md` §§ 3, 5, 8](spec.md#3-components))
  - Configurable system context budget (`max_provides: 5`, `max_requires: 8`, `max_owns_data: 4`) and `--context <component-id>` bounded slice export.
  - Physical bindings (`Component.source_paths`, `DataModel.schema_path`, `Evidence.subject_revision` / `timestamp` / `verifier`).
- **Architectural Waivers & Evidence Supersession** ([`spec.md` §§ 7–8](spec.md#7-risks))
  - Known violations (`layer-violation`, `dependency-cycle`, `shared-data-ownership`, `context-budget`, `verification-category`) allowed below `release` via `Risk.waives` with `mitigated_by` and `expires` (ISO date or stage); zero waivers permitted at `release` (`RISK-08`).
  - Evidence supports `model-check` (mandatory `bounds`) and explicit failure supersession (`supersedes`, `supersession_reason`).
- **Single Source of Truth & Formalization** ([`reference-implementation/`](reference-implementation/README.md))
  - Committed [`schema.json`](reference-implementation/schema.json) (`--schema`), drift-tested against `spec.md` format blocks.
  - `INVARIANTS` table in `metamodel.py`, drift-tested against `spec.md` § 11 and covered by [`corpus.yaml`](reference-implementation/corpus.yaml).
  - Alloy metamodel specification ([`metamodel.als`](reference-implementation/metamodel.als)).

---

## 3. Completed Roadmap

### Usable Development Process
- [x] Define gate inputs, review decisions, implementation handoffs and the change/reverification loop ([`CONSTITUTION.md`](CONSTITUTION.md), [`GREENFIELD.md`](GREENFIELD.md), [`BROWNFIELD.md`](BROWNFIELD.md)).
- [x] Walk a greenfield feature through discovery, design, verification, implementation and acceptance ([`examples/greenfield/README.md`](examples/greenfield/README.md)).
- [x] Walk a brownfield change through observed baseline, discrepancy review, characterization, failed-gate recovery, migration and acceptance ([`examples/brownfield/README.md`](examples/brownfield/README.md)).
- [x] Distinguish illustrative walkthrough decisions from actual execution evidence.

### Milestone 1: Normative Hardening & CI
- [x] Harden validator (`mitigated_by` references, unknown `--impact` IDs, requirement cycles, `avoidance`/`transfer` treatments and residual risk tracking).
- [x] Add sample `old.yaml` for `--compat`.
- [x] Enable `push`/`pull_request` triggers in `.github/workflows/metamodel.yml`.

### Milestone 2: Lifecycle, Exceptions & Binding Model
- [x] Specify and implement `status`, `--stage`, requirement `category`/`resolved_by`, evidence `model-check`/`bounds`/`supersedes`, `Capability.interface`, context budgets, and risk waivers (`waives`, `expires`).

### Milestone 3: Greenfield Workflow & Tooling
- [x] Split docs into `CONSTITUTION.md` + `GREENFIELD.md` with schema/invariant-citing prompts.
- [x] Implement `--context` slice export in `metamodel.py`.
- [x] Add traced end-to-end greenfield example and sample TLA+/Alloy models (`examples/greenfield/`).

### Milestone 4: Brownfield Onboarding Workflow & Tooling
- [x] Create `BROWNFIELD.md` defining the 4 adoption levels, 5-step reverse-discovery protocol, waivers and prompts.
- [x] Extend `--compat` with data contract compatibility for strangler migrations.
- [x] Add brownfield onboarding progression (`level0.yaml` → `level1.yaml` → `old.yaml` → `spec.yaml`).

### Milestone 5: Formalization
- [x] Commit `schema.json` with field descriptions and reference it from specs (step 1).
- [x] Assign stable invariant IDs in `spec.md` § 11 and cite them in validator errors and prompts (step 2).
- [x] Express stage-tagged checks as invariant data (`INVARIANTS`) driving `--stage` (step 3).
- [x] Drift-check `spec.md` format blocks against `schema.json` (step 4).
- [x] Add Alloy model of the metamodel (`reference-implementation/metamodel.als`) (step 5).
- [x] Add invalid spec corpus covering every invariant ID (`reference-implementation/corpus.yaml`) (step 6).

---

## 4. Optional Next Horizons

- **Toolkit Distribution**: specified in [`PACKAGING.md`](PACKAGING.md) and [`packaging.yaml`](packaging.yaml); wheel/source distribution, installed CLI (`system-spec`), test suite in `tests/`, and safe offline resource export implemented and verified.
- **Codebase & Schema Drift Verification**: `--check-drift --root` implemented for Python imports and JSON Schema properties; next: other languages (JS/TS) and schema formats (SQL DDL), field-type comparison.
- **Standards Crosswalk & Exporters** (see [`archive/BENCHMARK.md`](archive/BENCHMARK.md)): Mermaid diagram export implemented (`metamodel.py --diagram`); optional exporters for Backstage catalogs (brownfield inventory) and C4/Structurizr diagrams remain.
