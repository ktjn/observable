# System Specification

## 1. Introduction

A platform-independent metamodel for building scalable systems composed of independent components.

The metamodel defines structure, relationships, constraints, processes and verification. Implementation technologies are unrestricted.

A reference implementation shall demonstrate and validate the metamodel.

This document defines metamodel requirements. The [constitution](CONSTITUTION.md) defines shared development rules; the [greenfield](GREENFIELD.md) and [brownfield](BROWNFIELD.md) tracks define their workflows; [skills](.github/skills/) describe individual techniques.

Blocks marked **Current reference format** define the serialization shared by tracks, skills and tooling; its limitations do not relax these requirements. Prototype behavior and limitations: [reference implementation](reference-implementation/README.md).

## 2. Systems

A system is a composition of components.

- Systems shall define their components and relationships.
- Systems may organize components into layers with directional dependency constraints.
- Systems may group components into domains (bounded contexts), orthogonal to layers.
- Dependencies shall be resolved through capabilities.
- Systems shall define requirements, risks and quality attributes.
- System composition shall be validated for completeness and compatibility.

Layer policies shall identify allowed dependency directions and whether same-layer or non-adjacent dependencies are permitted. Components subject to a layer policy shall belong to declared layers.

Domains group cohesive components; a domain may span layers and a layer may hold several domains, so each component sits at one (layer, domain) cell. Layers set dependency direction; domains set which groups may depend on each other: a cross-domain dependency shall be declared in the consuming domain's `uses` (`DOM-02`), and `uses` shall be acyclic (`DOM-03`). Sibling groups that must not depend on each other are domains in the same layer without `uses` between them. Data models belong to their owner's domain.

Logical capability dependencies and source-code dependencies shall be distinguished. A domain component may consume an infrastructure-provided capability through a technology-independent contract; its domain logic shall not depend on infrastructure implementation types.

**Current reference format:** `SystemDef` (`system`) contains `id`, `version`, `status`, `layers` (ordered from highest to lowest), `domains` (`Domain`: `id`, `description`, `uses`), `external_sources`, `imports` (`SpecImport`: `id`, `path`, `version`, `capabilities`, optional `sha256`), `quality_attributes` and optional `context_budget` (`max_provides`, `max_requires`, `max_owns_data`).

## 3. Components

A component is an independent logical unit providing one or more capabilities.

- Components shall have stable identities, versions and lifecycle statuses.
- Components may depend on zero or more capabilities provided by other components.
- Components shall define explicit boundaries, contracts and consistency guarantees.
- Components may own, consume, transform and expose data.
- Components shall define requirements, risks and verification evidence.
- Components shall remain within declared context-budget limits.
- Components may bind to physical source paths or modules for drift detection while remaining independent of deployment and implementation technologies.

Context budgets shall bound component size by limiting provided capabilities, required capabilities and owned data models (default limits: `max_provides: 5`, `max_requires: 8`, `max_owns_data: 4`, overridable per system and, for composition roots, per component via `Component.context_budget`). Components exceeding the budget shall be split or covered by an active waiver (§ 7).

Artifact bindings (`source_paths`) map a component to its implementation directories or modules so tools can verify that source-level imports conform to declared capability and layer dependencies. A component can be developed and verified on its own: `test_paths` names the directories or files holding its own tests, test evidence for it shall come from there (`COMP-08`), and tests are subject to the same import-conformance check as source, and each required capability has a fake (`fakes`: capability ID → test double under `test_paths`, `COMP-09`). Tests shall not import other components; they depend on the fakes, so the component builds and tests from its slice alone. A capability that any component fakes shall have a verified `contract`-category requirement with passing evidence (`COMP-10`) so the fake's assumptions are tested at the provider; that a fake matches those tests is a review obligation. A component with `test_scope: integration` (assembly or end-to-end tests) is exempt from the fake obligation: its tests may also use real providers of capabilities it requires. Drift tools shall list what they cannot check, including components without `source_paths`.

An external component (`external: true`) is a provider outside the system, such as a platform or third-party API, modeled for its contracts: it only provides capabilities and declares no `layer`, `domain`, `requires`, `test_paths` or `fakes` (`COMP-11`), is exempt from context budgets, and its `source_paths`, if any, identify it as a dependency target without being checked themselves. A dependency mediated at runtime rather than in code (e.g. a cross-module write through a transaction platform) names its mediating required capability in `via` (required capability → mediating capability, `REF-05`); tools treat such edges as unverifiable by source analysis.

**Current reference format:** `Component` contains `id`, `version`, `status`, `layer`, `domain`, `consistency`, `provides`, `requires`, `owns_data`, `bindings`, `source_paths`, `test_paths`, `fakes`, `test_scope`, `external` and `via`.

## 4. Capabilities

A capability describes functionality independently of its implementation.

- Components shall declare provided and required capabilities.
- Capabilities shall define contracts, including preconditions, postconditions, invariants, failure modes and interface bindings.
- Capabilities shall support lifecycle status, versioning and compatibility validation.
- Required capabilities shall resolve to compatible providers.

Interface contracts shall specify for each externally or cross-component exposed capability:

- Operation identifier or endpoint and interaction mode (`sync` or `async`).
- Request and response `DataModel` references.
- Idempotency, authentication/authorization requirements and failure-mode to status-code mappings.

Required capabilities shall resolve unambiguously to providers other than the consuming component. Compositions with multiple candidates shall define provider selection explicitly; declaration order shall not determine selection. Dependency cycles are forbidden unless covered by a time-bounded waiver (§ 7) below the `release` stage.

**Current reference format:** capabilities use `id`, `version`, `status`, `preconditions`, `postconditions`, `invariants`, `failure_modes` and optional `interface` (`operation`, `mode`, `request`, `response`, `idempotent`, `idempotency_strategy`, `auth`, `errors`). `request` and `response` reference a `DataModel` ID or list of IDs (`REF-13`). `errors` maps every failure mode to a status code or event; capabilities required by another component need an `interface`. In-process capabilities (e.g. Rust traits) with no authentication boundary set `auth: none-in-process`. Contract clauses are lists of strings whose meaning shall be defined through requirements and acceptance criteria.

```yaml
capabilities:
  - id: reserve-inventory
    version: "1.0"
    preconditions: [product_exists, quantity_positive]
    postconditions: [reservation_created, stock_decremented]
    invariants: [stock_never_negative]
    failure_modes: [insufficient_inventory, product_not_found]
    interface: { operation: POST /v1/reservations, request: stock-item, idempotent: true, auth: service-token, errors: { insufficient_inventory: 409, product_not_found: 404 } }
```

`Component.provides` contains capability IDs. `Component.requires` contains IDs or version-constrained references such as `reserve-inventory@^1.0` (§ 10). `Component.bindings` maps a required capability ID to the selected provider component. The format holds one version per capability ID.

## 5. Data Models

Components may own, consume, derive and expose data.

- Data models shall define ownership, structure and contracts.
- External data dependencies shall be tracked.
- Derived and re-exposed data shall preserve lineage.
- Data classifications and constraints shall propagate through transformations.
- Data contracts shall support versioning and compatibility validation.

Ownership declarations shall agree: a model's `owner` shall identify an existing component, and that component's `owns_data` shall include the model. Lineage references shall identify existing models; external sources shall be explicitly registered. Data models may bind to a physical schema artifact (`schema_path`) for drift and compatibility checks.

- Classification order is `public < internal < confidential < restricted`. Missing classification defaults to `unknown`, never implicitly to `public`.
- At inventory, `unknown` requires an active `classification_review` requirement and an explicit conservative handling policy in the intake record. Unknown is not a rank and cannot authorize declassification; resolve it before architecture acceptance, including for lineage sources.
- A derived model's classification shall be at least as restrictive as every source in `derived_from` unless authorized declassification is declared.
- Authorized declassification (`declassified_by`) shall reference a verified requirement justifying the transformation (e.g. anonymization, aggregation or field stripping).

**Current reference format:** `DataModel` contains `id`, `version`, `owner`, `fields`, `classification`, `derived_from` and optional `classification_review`, `namespace`, `storage_role`, `external_source`, `schema_path` and `declassified_by`. `fields` maps names to type descriptions; `external_source` refers to an entry in `system.external_sources`. `owner` is the logical owning component, which need not match the package or crate hosting the schema (`schema_path` locates it). Capabilities of other systems are not modeled; register that system in `external_sources` and record its boundary in the adoption register. An unresolved external provider shall not be represented as a resolved internal dependency.

## 6. Requirements

Requirements specify expected behavior and constraints.

- Requirements shall have stable identities, lifecycle statuses and acceptance criteria.
- Requirements may apply to systems, components, capabilities or data models.
- Requirements may depend on, refine or conflict with other requirements.
- Requirements shall support functional and non-functional properties and map to verification categories.
- Every mandatory active requirement shall have a verification obligation.

- `depends_on` identifies prerequisite requirements; `refines` identifies requirements whose obligations are elaborated; `conflicts_with` identifies incompatible obligations.
- A composition shall identify its active requirements (`status` in `draft`, `specified`, `verified`; `deprecated` requirements are inactive).
- Conflicts between active requirements shall be rejected unless resolved via `resolved_by` referencing an active requirement whose criteria and evidence supersede or reconcile the conflicting obligations.
- Setting a requirement to optional shall not resolve a conflict or remove its dependencies.

**Current reference format:** requirements use `id`, `version`, `target`, `kind`, `mandatory`, `criteria`, `depends_on`, `refines`, `conflicts_with`, `status`, `category` and `resolved_by`. `kind` is `functional` or `non-functional`; `mandatory` defaults to `true`; `category` is one of `functional`, `semantic`, `contract`, `serialization`, `concurrency`, `performance`, `scalability`, `resilience`, `security`, `observability`.

## 7. Risks

Risks describe potential failures, undesirable outcomes or tracked architectural exceptions.

- Components and systems shall identify and assess risks.
- Risks shall have explicit treatments: mitigation, acceptance, avoidance or transfer.
- Mitigations shall be expressed through verifiable requirements.
- Residual risks shall be tracked and explicitly accepted where applicable.
- Architectural exceptions (waivers) shall be bound to risks with remediation requirements and expiration bounds.

Mitigation acceptance shall require valid evidence for the mitigating requirements, including optional requirements used as mitigations. A treatment label alone shall not establish that a risk has been addressed.

A waiver (`waives`) allows a known brownfield violation (layer or domain violation, dependency cycle, shared data ownership, context-budget overrun or missing verification category) below the `release` stage when:

- `treatment` is `mitigation` or `acceptance` and `mitigated_by` names at least one remediation requirement.
- `expires` specifies an ISO-8601 date (`YYYY-MM-DD`) or target stage (`architecture`, `verification`, `release`) that has not passed.
- No waivers are permitted at `--stage release`.

**Current reference format:** risks use `id`, `target`, `treatment`, `mitigated_by`, `residual_accepted`, `transferred_to`, `waives` and `expires`. `waives` is one of `layer-violation` (`COMP-05`), `domain-violation` (`DOM-02`), `dependency-cycle` (`COMP-06`, `DOM-03`), `shared-data-ownership` (`DATA-03`), `context-budget` (`COMP-03`), `verification-category` (`REQ-01`) or the corresponding invariant ID; a waiver covers violations whose subjects include the risk `target`, or all of that kind when `target` is the system. Avoidance requires verified mitigating requirements and no residual risk; transfer requires a counterparty (`transferred_to`) and accepted residual risk.

## 8. Verification

Verification establishes evidence that requirements are satisfied.

- Requirements shall be verified through tests, formal proofs, bounded model checks, analysis, inspection or other justified methods.
- Evidence shall be traceable to requirements and applicable versions.
- Evidence shall record results, context, subject revision and provenance.
- Changes shall invalidate affected evidence.
- Compliance shall be determined through defined verification rules.

Valid evidence shall identify:

- The requirement and version, verification method (`test`, `proof`, `model-check`, `analysis`, `inspection`) and result.
- The verified subject (`subject_id`) and its version or immutable revision (`subject_revision`).
- Execution time (`timestamp`), verifier or tool identity (`verifier`), conditions, assumptions and bounds/scope (mandatory for `model-check`).
- Retrievable supporting artifacts and their provenance.

Evidence shall cover the applicable acceptance criteria. A passing result shall not override an unresolved failure for the same obligation and applicable subject unless the new evidence explicitly declares `supersedes` (referencing the failed evidence ID) and `supersession_reason`.

Changes to requirements, contracts, components, data models or relevant dependencies shall trigger impact analysis. Affected evidence shall be invalidated and the affected obligations reverified; an unchanged requirement version alone shall not justify reuse.

**Current reference format:** `Evidence` contains `id`, `requirement_id`, `requirement_version`, `method`, `passed`, `provenance` and optional `bounds` (required for `model-check`), `subject_id` (defaults to `Requirement.target` when omitted), `subject_revision`, `subject_version`, `artifacts`, `skipped`, `timestamp`, `verifier`, `environment` (where it ran, e.g. host or CI job), `criteria` (requirement criteria it covers; empty means the whole requirement), `supersedes` and `supersession_reason`.

**Current reference format:** `Spec` has optional `subject_bindings`, mapping subject IDs to `SubjectBinding` records containing `version`, `revision` and non-empty `artifacts` (repository-relative source file paths mapped to exact-byte lowercase SHA-256 digests). `Evidence.artifacts` uses the same path/digest format for retrievable execution outputs. `skipped` lists omitted check names/reasons; evidence with skips does not verify a requirement and cannot pass release. `SubjectBinding.version` and `Evidence.subject_version` shall match the modeled subject version; `Evidence.subject_revision` shall match its reviewed binding's source `revision`. A stale or absent binding fails release when `subject_version` is supplied. Existing evidence without a binding or `subject_version` retains the legacy convention that `subject_revision` equals the modeled version.

`provenance` takes the form `<artifact>::<name>` or `<tool>::<name>`. For `test` evidence the artifact should be a repository-relative file path (`tests/checkout/test_checkout.py::test_place_order`); a bare tool label (`pytest::test_x`) names no file, so it cannot be checked against `--check-artifacts` or a component's `test_paths` (`COMP-08`). Package-level runners (Cargo `nextest -p`, `go test ./pkg`) use selectors instead: a `test_paths` or `fakes` entry containing `::` is a selector (`nextest::-p scalable-fm`); provenance or a fake matches it when equal or when it continues the selector with `::` (`nextest::-p scalable-fm::test_x`). Selectors name no files, so artifact and import checks skip them and `--check-drift` lists them as unchecked.

Bindings are review inputs, not proof of a Git commit or source-to-build correspondence. Optional `--check-artifacts --root <repository>` checks declared source/schema existence, source/evidence file hashes and containment within that explicit repository root. It never executes artifacts or resolves remote provenance. Hashes cover exact file bytes, including checkout line endings. Unhashed source paths and schema paths receive existence checks only; schema/model semantic drift still requires independent verification beyond the optional `--check-drift --root <repository>`, which parses Python imports between components' `source_paths` (an undeclared dependency is one with no `requires` capability or `bindings` entry reaching the imported component) and compares top-level JSON Schema `properties` with `fields` names; other languages and schema formats are reported as unchecked or not inspected. Ordinary validation does not inspect files. A handoff retains applicable subject bindings in its context slice; evidence remains in the full specification/review record.

Acceptance shall include structural validation, evidence review, applicable architecture checks and risk disposition. Formal verification shall include reviewed assumptions, checked properties and correspondence to the implementation where applicable. A successful metadata check alone shall not establish system acceptance.

## 9. Scalability and Quality

Systems shall define measurable quality attributes.

- Attributes may include scalability, performance, reliability, security and maintainability.
- Quality requirements shall define measurable constraints and acceptance criteria.
- Verification shall establish compliance under specified conditions.
- The metamodel shall not prescribe implementation mechanisms.

## 10. Lifecycle and Evolution

Systems, capabilities, components and requirements shall support controlled evolution across lifecycle states and validation stages.

- Entities shall progress through `status` states: `draft` → `specified` → `verified` → `deprecated`.
- Contracts, requirements and data models shall be versioned.
- Changes shall support dependency and impact analysis.
- Compatibility rules shall govern composition and replacement.
- Affected requirements shall be reverified following changes.
- Deprecation and removal shall be explicitly managed.

Compatibility shall preserve existing consumers' obligations and guarantees:

- Backward-compatible capability replacements shall not strengthen preconditions, weaken postconditions or remove required invariants.
- Failure-mode changes shall be assessed for their effect on consumers.
- Data-contract changes shall preserve field types, classifications and constraints required by consumers, or be handled as explicit breaking changes with a major version bump.
- Versioning policies shall identify breaking changes and the required migration. Matching a version range shall not substitute for contract verification.
- Impact analysis shall follow transitive capability and data dependencies and include affected system-level requirements and evidence.

**Current reference format:** capability references are an unconstrained ID or `id@^version` (same major version, at least the given version; e.g. `@^1.0` accepts `1.2`, rejects `0.9` and `2.0`). This is not a general semantic-versioning standard.

## 11. Metamodel Validation

The metamodel shall define machine-verifiable invariants evaluated progressively across five monotonic validation stages (`--stage`):

| Stage | Required Entity `status` | Enforced Invariants | Waivers Allowed |
|---|---|---|---|
| `inventory` | `draft`+ | Unique IDs; valid target/reference syntax; registered `system`, `external_sources`, component stubs (`provides` optional) | Yes |
| `discovery` | Capabilities `specified`+ | `inventory` + every capability defines non-empty contract clauses (`preconditions` or `postconditions`, `invariants`, `failure_modes`) | Yes |
| `architecture` | Capabilities & components `specified`+ | `discovery` + every component provides ≥1 capability; required capabilities resolve; layer and domain rules, cycle checks, data ownership, lineage, classification, interface contracts and context budgets hold | Yes (via `Risk.waives`) |
| `verification` | Active requirements `specified`+ | `architecture` + every capability has requirements; all 10 verification categories covered; requirement dependency/refinement acyclic; no unresolved conflicts | Yes (via `Risk.waives`) |
| `release` | All active entities `verified` (or `deprecated`) | `verification` + every mandatory and mitigating requirement has passing, current, non-superseded `Evidence`; all risks disposed (§ 7); zero active or expired waivers | No |

Stages may be entered sequentially (greenfield Gates 1–4: `discovery` → `architecture` → `verification` → `release`) or via brownfield adoption levels (Level 0 `inventory` → Level 1 `architecture` → Level 2 `verification` → Level 3 `release`, skipping standalone `discovery` because `architecture` enforces capability contract presence (`CAP-01`)).

Each invariant has a stable ID that validators shall report with every violation. An invariant applies from its stage onward; at `release`, all hold without exception. A waivable invariant may be suspended below `release` by an active `Risk.waives` of the listed kind (§ 7). `COMPAT` invariants apply between two versions of a specification (§ 10).

| ID | Stage | Waiver | Rule |
|---|---|---|---|
| `REF-01` | `inventory` | — | IDs are unique within each entity type. |
| `REF-02` | `inventory` | — | Requirement and risk targets resolve to exactly one system, component, capability or data model. |
| `REF-03` | `inventory` | — | Provided capabilities exist. |
| `IMP-01` | `inventory` | — | Imports have unique IDs; qualified capability references resolve to a pinned import and are provided only by external components. |
| `REF-04` | `inventory` | — | Owned data models exist. |
| `REF-05` | `inventory` | — | Bindings and via entries name required capabilities. |
| `REF-06` | `inventory` | — | Requirement relationships reference existing requirements. |
| `REF-07` | `inventory` | — | Evidence references an existing requirement. |
| `REF-08` | `inventory` | — | Evidence subjects exist. |
| `REF-09` | `inventory` | — | Risk mitigations reference existing requirements. |
| `REF-10` | `inventory` | — | Data model owners are existing components. |
| `REF-11` | `inventory` | — | Lineage references existing data models. |
| `REF-12` | `inventory` | — | Declassification references an existing requirement. |
| `REF-13` | `inventory` | — | Interfaces reference existing data models. |
| `REF-14` | `inventory` | — | Subject bindings name existing subjects and non-empty source revisions. |
| `DATA-01` | `inventory` | — | External data sources are registered. |
| `DATA-02` | `architecture` | — | Owners list their data models in owns_data. |
| `DATA-03` | `architecture` | `shared-data-ownership` | Each data model has exactly one owning component. |
| `DATA-04` | `architecture` | — | Derived data keeps its sources' classification unless declassified by an active requirement. |
| `DATA-05` | `architecture` | — | Data classifications are known before architecture acceptance. |
| `DATA-06` | `inventory` | — | Unknown classifications name an active review requirement; declared reviews resolve. |
| `EV-01` | `inventory` | — | Model-check evidence states its bounds. |
| `EV-02` | `inventory` | — | Superseding evidence passes and supersedes failed evidence of the same requirement. |
| `EV-03` | `inventory` | — | Supersession states a reason. |
| `EV-04` | `release` | — | Release evidence matches the subject version and any declared source binding, with timestamp and verifier. |
| `EV-05` | `release` | — | Release evidence has no skipped checks. |
| `EV-06` | `inventory` | — | Evidence criteria name criteria of the verified requirement. |
| `RISK-01` | `inventory` | — | Waivers require mitigation or acceptance, mitigating requirements and an expiry. |
| `RISK-02` | `inventory` | — | Waivers have not expired. |
| `RISK-03` | `release` | — | Mitigated and avoided risks name mitigating requirements. |
| `RISK-04` | `release` | — | Mitigating requirements are verified. |
| `RISK-05` | `release` | — | Avoided risks carry no residual risk. |
| `RISK-06` | `release` | — | Transferred risks name the receiving party. |
| `RISK-07` | `release` | — | Accepted and transferred risks record residual acceptance. |
| `RISK-08` | `release` | — | No waivers remain. |
| `STAT-01` | `discovery` | — | Capabilities are at least specified. |
| `STAT-02` | `architecture` | — | Components are at least specified. |
| `STAT-03` | `verification` | — | Active requirements are at least specified. |
| `STAT-04` | `release` | — | System, capabilities, components and active requirements are verified or deprecated. |
| `CAP-01` | `discovery` | — | Capabilities define preconditions or postconditions, invariants and failure modes. |
| `CAP-02` | `architecture` | — | Capabilities required across components declare an interface. |
| `CAP-03` | `architecture` | — | Interfaces define operation, idempotent, auth and map errors to failure modes. |
| `CAP-04` | `verification` | — | Active capabilities have requirements. |
| `COMP-01` | `architecture` | — | Components provide at least one capability. |
| `COMP-02` | `architecture` | — | Component layers are declared. |
| `COMP-03` | `architecture` | `context-budget` | Components fit the context budget. |
| `COMP-04` | `architecture` | — | Required capabilities resolve to exactly one compatible provider. |
| `COMP-05` | `architecture` | `layer-violation` | Components depend only on the same or lower layers. |
| `COMP-06` | `architecture` | `dependency-cycle` | Component dependencies are acyclic. |
| `COMP-07` | `architecture` | — | Active components do not require deprecated capabilities. |
| `COMP-08` | `verification` | — | Test evidence for a component that declares test_paths comes from files or selectors under them. |
| `COMP-09` | `verification` | — | A unit-scoped component that declares test_paths has a fake under them (file or selector) for each required capability. |
| `COMP-10` | `release` | — | A capability that a consumer fakes has a verified contract requirement. |
| `COMP-11` | `architecture` | — | External components declare no layer, domain, requires, test_paths or fakes. |
| `DOM-01` | `architecture` | — | Component domains and domain uses are declared. |
| `DOM-02` | `architecture` | `domain-violation` | Cross-domain dependencies are declared in Domain.uses. |
| `DOM-03` | `architecture` | `dependency-cycle` | Domain uses are acyclic. |
| `REQ-01` | `verification` | `verification-category` | Active requirements cover all verification categories. |
| `REQ-02` | `verification` | — | Requirement dependencies and refinements are acyclic. |
| `REQ-03` | `verification` | — | Requirement conflicts are resolved by an active requirement. |
| `REQ-04` | `release` | — | Obligated requirements have no unsuperseded failing evidence for their version. |
| `REQ-05` | `release` | — | Obligated (mandatory, declassifying, resolving) requirements have passing evidence for their version. |
| `REQ-06` | `release` | — | Dependencies of obligated and mitigating requirements are verified. |
| `REQ-07` | `verification` | — | Requirement kind matches its verification category. |
| `COMPAT-01` | change | — | Preconditions are not strengthened. |
| `COMPAT-02` | change | — | Postconditions are not removed. |
| `COMPAT-03` | change | — | Invariants are not removed. |
| `COMPAT-04` | change | — | Failure modes are not added. |
| `COMPAT-05` | change | — | Data model fields are not removed. |
| `COMPAT-06` | change | — | Data model field types are unchanged. |
| `COMPAT-07` | change | — | Data model classification is unchanged. |
| `COMPAT-08` | change | — | Breaking changes bump the major version. |
| `COMPAT-09` | change | — | Capabilities are deprecated before removal. |

Acceptance evidence shall also match the applicable subject versions, and lineage shall preserve constraints.

Validation results shall distinguish schema checks, composition checks and verification of the described system. Unsupported checks and unresolved obligations shall be reported explicitly.

**Current reference format:** `metamodel.py --stage <stage>` (default `release`) evaluates the stage's invariants; `--context <component-id>` prints the component's spec slice.

## 12. Reference Implementation

A reference implementation shall demonstrate:

- Declarative specifications of systems, components and capabilities.
- Dependency and capability resolution.
- Requirement, risk and data traceability.
- Contract and compatibility validation.
- Verification evidence collection and evaluation.
- Change-impact analysis.
- Composition of one specification from fragment files, so components, teams or agents own separate files: a root file keeps `system` and may list `include` patterns (relative to the root, no absolute paths or `..`); each matched file is a fragment holding any other section (`capabilities`, `components`, `data_models`, `requirements`, `risks`, `evidence`, `subject_bindings`). Lists are concatenated in sorted path order, `subject_bindings` keys shall be unique, fragments shall not contain `system` or `include`, and the resolved specification is validated as one document (duplicate IDs remain `REF-01`). The root still declares the required `capabilities` and `components` keys (they may be empty). Artifact paths stay relative to the repository root, not the fragment.
- Automated metamodel conformance checks.

The reference implementation shall not constrain alternative implementations. A system may reference capabilities of another system's specification without copying them: `system.imports` pins the other spec (`path`, `version`, and each referenced capability's version, optionally the file `sha256`), and an `external` component provides `<import-id>:<capability>`, which consumers require as `<import-id>:<capability>@^<version>`. The contracts (interfaces, data models) stay in the imported spec; `--check-imports --root <repository>` verifies the pins against it. A passing validator run shall not establish release acceptance, which still requires the evidence and obligations in §§ 7–11.

The [Python reference implementation](reference-implementation/README.md) is a partial prototype; its README documents usage, behavior and limitations. Its JSON Schema ([`schema.json`](reference-implementation/schema.json), from `--schema`) is the machine-readable form of the current reference format.
