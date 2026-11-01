# Benchmark: Metamodel vs. Existing Standards

Compares the [`spec.md`](../spec.md) metamodel and [`metamodel.py`](../reference-implementation/metamodel.py) with established standards and tools.

## Findings

- **Custom metamodel**: entities and invariants are original; no standard is cited or derived from.
- **Standard building blocks only**: Pydantic (types → JSON Schema via `--schema`), YAML, ISO-8601 dates (`expires`), npm-style caret ranges (`id@^version`, explicitly not general SemVer, `spec.md` § 10).
- **Familiar concepts, own vocabulary**: most entities have close counterparts (below), but `spec.md` never names them, so readers and LLMs cannot reuse prior knowledge.
- **Distinctive combination**: no single standard unites capability contracts, requirement → evidence traceability, risk treatment, data classification/lineage, staged validation with waivers and LLM context budgets in one compact, machine-checked file.

## Entity Crosswalk

| Metamodel | Closest standard concept | Fit |
|---|---|---|
| `system`, `layers` | C4 software system; ArchiMate layers; Backstage `System` | ✅ |
| `Component` (`provides`/`requires`) | UML component with provided/required interfaces; Backstage `providesApis`/`consumesApis` | ✅ |
| `Capability` contract (`preconditions`, `postconditions`, `invariants`) | Design by Contract `require`/`ensure`/`invariant`; OCL `pre`/`post`/`inv` | ✅ same terms |
| `failure_modes` | OpenAPI error responses; FMEA failure modes | 🟡 names only |
| `interface` | OpenAPI 3.1, AsyncAPI 3.0 | 🟡 custom fields |
| `DataModel` `fields` | JSON Schema; Avro/Protobuf | 🟡 type strings only |
| `classification` | ISO/IEC 27001:2022 A.5.12 information classification | 🟡 levels not standardized |
| `derived_from`, `external_source` | OpenLineage dataset lineage | 🟡 static, design-time |
| `Requirement` | ISO/IEC/IEEE 29148 requirement; SysML v2 `requirement def` | 🟡 lacks rationale, source, priority |
| `depends_on`, `refines`, `conflicts_with` | SysML `deriveReqt`/`refine`; OSLC RM links | ✅ |
| `Risk.treatment` | ISO 31000 treatment options | 🟡 ISO uses "sharing", not "transfer" |
| `Evidence` | SysML v2 `verify`; OSLC QM test result; in-toto attestation; GSN solution | 🟡 flat, no argument |
| `Evidence.method` | Systems-engineering IADT (inspection, analysis, demonstration, test) | 🟡 adds `proof`, lacks `demonstration` |
| `status` lifecycle (specified) | Backstage `lifecycle`; 29148 requirement attributes | ✅ |
| Stages, waivers (`waives`, `expires`) | Architecture conformance tools (ArchUnit, import-linter) baselines/exceptions | 🟡 no standard |
| Context budget | — | ❌ unique |

## Standards Compared

| Standard / tool | Covers | Missing vs. metamodel | Use here |
|---|---|---|---|
| **SysML v2** (OMG) | Parts, ports, requirements, `satisfy`/`verify`, textual notation, API | Data classification, waivers, LLM budgets; heavy toolchain | Vocabulary reference; not adopted |
| **UML + OCL** (OMG) | Provided/required interfaces; `pre`/`post`/`inv` | Requirements, evidence, risk | Name contract semantics after OCL |
| **ArchiMate 3.2** (The Open Group) | Layers, application components/services, motivation (requirement, constraint, assessment) | Evidence, contracts, validation | Layer vocabulary |
| **C4 / Structurizr DSL** | System → container → component views | Contracts, requirements, evidence | Optional diagram export |
| **Backstage catalog** | `System`/`Component`/`API`, `providesApis`/`consumesApis`, `lifecycle` | Contracts, requirements, evidence, data rules | Optional catalog export (brownfield inventory) |
| **ISO/IEC/IEEE 29148:2018** | Requirement quality, attributes, traceability | Machine-checked rules | Requirement attributes |
| **ISO 31000:2018** | Risk process and treatment options | Link to requirements/evidence | Treatment vocabulary |
| **GSN / OMG SACM** | Claim → argument → evidence | Architecture, data | Evidence structure if arguments are needed |
| **OSLC RM/QM** (OASIS) | Requirement ↔ test ↔ result links (RDF) | Architecture, risk | Link semantics |
| **in-toto / SLSA** | Attestation: subject digest, predicate type, builder | Requirements | `subject_revision`, `verifier` fields |
| **OpenAPI / AsyncAPI** | Sync/async interface contracts | Requirements, evidence | Reference instead of custom `interface` fields |
| **OpenLineage** | Runtime dataset/job lineage | Classification rules | Drift check of `derived_from` |
| **Doorstop, Sphinx-needs, StrictDoc** | Requirements-as-code with traceability checks | Architecture, data, risk | Prior art for formalization |
| **OPA / Rego** | Policy-as-data over JSON/YAML | Domain model | Option for invariants as data (formalization step 3) |

## Gaps Revealed

- **Assurance structure**: evidence is a flat list; GSN/SACM add the argument linking evidence to claims.
- **Requirement attributes**: 29148 rationale, source and priority absent.
- **Evidence provenance**: `provenance` is free text; in-toto uses subject digests and typed predicates.
- **Interfaces**: custom `interface` fields duplicate OpenAPI/AsyncAPI.
- **Vocabulary drift**: `transfer` (ISO 31000 "sharing"); `proof` vs. IADT `demonstration`.

## Recommendations

1. Add a short crosswalk to `spec.md` naming the closest standard per entity (low cost; helps humans and LLMs).
2. Align vocabulary where cheap: note `transfer` = ISO 31000 sharing; consider `demonstration` as an `Evidence.method`.
3. Reference OpenAPI/AsyncAPI documents from `interface` instead of re-specifying operations.
4. Align evidence subject fields with in-toto (subject digest, verifier, predicate type).
5. Keep the custom metamodel; do not adopt SysML v2 or ArchiMate wholesale (loses compactness and LLM context fit).
6. Optional exporters: Backstage catalog (brownfield inventory), Structurizr DSL (diagrams).
