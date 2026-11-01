# Reference Implementation

Validates [spec.md](../spec.md) with Python, Pydantic, YAML, and pytest.

- Validate spec: `uv run python metamodel.py example.yaml [--stage inventory|discovery|architecture|verification|release]` (default `release`)
- Export schema: `uv run python metamodel.py --schema > schema.json` (committed; regenerate on model change, a test checks drift). Specs reference it via a `# yaml-language-server: $schema=…` modeline.
- Impact analysis: `uv run python metamodel.py example.yaml --impact order`
- Compatibility: `uv run python metamodel.py new.yaml --compat old.yaml`
- Context slice: `uv run python metamodel.py example.yaml --context checkout`
- Diagram: `uv run python metamodel.py example.yaml --diagram` — Mermaid flowchart (layers, resolved capability dependencies, data ownership/lineage, status styling); renders natively on GitHub/Markdown
- JSON output: add `--json`
- Optional local artifact integrity: `--check-artifacts --root <repository>` checks declared source/schema paths and source/evidence SHA-256 hashes without executing or fetching artifacts. An explicit root is required; ordinary validation remains metadata-only.
- Optional drift check: `--check-drift --root <repository>` parses Python sources (never executes them) and reports an import between components with no `requires` capability or `bindings` entry reaching the imported component, and JSON Schema top-level `properties` that differ from `DataModel.fields`. Unsupported schema formats are listed as `unchecked`.
- Run tests: `uv run pytest` (incl. `corpus.yaml`: one or more invalid patches of `example.yaml` per invariant ID; examples are the valid corpus)
- Docker: `docker build -t metamodel . && docker run --rm metamodel [python metamodel.py ... | pytest]`
- CI & remote: `.github/workflows/metamodel.yml` runs `pytest` automatically on `push`/`pull_request` and runs either script in Docker when dispatched manually (`script`, `args`)

Partial prototype; the serialization format is defined in [spec.md](../spec.md) (**Current reference format**).

## Validator Behavior

- **Schema**: unknown fields are rejected (`extra="forbid"`); `status` defaults to `draft`.
- **Invariants**: `INVARIANTS` maps each ID (spec.md § 11) to its stage, waiver kind and rule; errors are prefixed `[ID]`, and a test keeps the table in sync with spec.md.
- **Stages** (`--stage`): each invariant runs at its stage and later.
- **Waivers**: valid below `release` when `treatment` is `mitigation`/`acceptance`, `mitigated_by` is non-empty and `expires` (date or stage) is not passed (stage expiry: current stage after `expires`). `Risk.waives` accepts either the waiver kind or its invariant ID (`DATA-03`, `COMP-03`, `COMP-05`, `COMP-06`, `REQ-01`). Active waivers are listed as `waivers` in `--json` output.
- **Interfaces**: capabilities required by another component need `interface`; `operation`, `idempotent` and `auth` must be defined; `request`/`response` must name data models; `errors` keys must equal `failure_modes`.
- **Requirements**: `deprecated` requirements are inactive (no coverage, conflicts or evidence obligations); `Requirement.kind` must match its `category` (`REQ-07`); a conflict is resolved when either side's `resolved_by` names an active requirement; mandatory, mitigating, resolving and declassifying requirements need evidence and verified dependencies at `release`; `declassified_by` relaxes classification propagation only when it names an active requirement.
- **Layers**: dependencies allowed within a layer or toward any lower layer (incl. non-adjacent); with non-empty `system.layers`, every component must declare one. Adjacency and source-code dependency rules need separate verification.
- **Resolution**: excludes the consuming component; rejects ambiguous providers without a binding, dependency cycles, and active components requiring deprecated capabilities (`COMP-07`); one version per capability ID.
- **Versions** (`_ver_ok`): dot-separated integers; equal major, remaining segments ≥ minimum; non-numeric never matches. No segment normalization, major-zero rules, prereleases or general ranges.
- **Risks**: mitigating requirements need valid evidence; `avoidance` also needs verified `mitigated_by` and no `residual_accepted`; `acceptance` and `transfer` need `residual_accepted: true`, and `transfer` needs `transferred_to`. Accepted residual risks are listed as `residual_risks` in `--json` output. Mitigation effectiveness is not checked.
- **Classification**: missing classification is `unknown`; inventory requires an active `classification_review` requirement. Unknown classifications block architecture and later stages, including when lineage is declassified (`DATA-05`, `DATA-06`). Existing inputs that omitted classification must now state a reviewed value or an inventory review obligation.
- **Evidence**: requires a passing, unskipped record for `(requirement_id, requirement_version)`; rejects unsuperseded failures. `model-check` requires `bounds`; `subject_id` must resolve. At release, `timestamp` and `verifier` are required. Explicit `subject_version` / `subject_revision` pairs match `subject_bindings` and current modeled versions (`EV-04`); legacy version-valued `subject_revision` remains supported when no binding/pair is declared. Skips block release (`EV-05`). Optional `artifacts` hashes verify local file integrity only; criterion coverage, Git authenticity and build correspondence remain review obligations.
- **Compatibility** (`check_compatibility`, `check_data_compatibility`, `--compat`): compares clauses as string sets; reports added preconditions, removed postconditions/invariants, added failure modes, capabilities removed without prior `status: deprecated`, removed data models, data models with removed fields, changed field types or classification, and missing major bumps. No clause semantics, interface or constraint comparison.
- **Context** (`--context`): the component, its provided/required capabilities, owned and interface data models, applicable source bindings, and requirements/risks targeting them or the system; evidence excluded.
- **Impact** (`impact_analysis`, `--impact`): follows capability, ownership and lineage dependencies transitively; reports affected components, capabilities, data models, requirements (incl. system-level and dependent) and evidence (by requirement or `subject_id`). Stored evidence is not changed.

## Limitations

- `consistency` and `quality_attributes` parsed only. Source/schema paths are checked for existence/containment with `--check-artifacts`; `--check-drift` covers Python imports and JSON Schema property names only (no other languages or formats, field types, `required`, `$ref`/`$defs`).
- No automatic evidence invalidation; impact analysis only reports it.
- Results separate schema from metadata errors; metadata errors carry invariant IDs but not stage labels.
- JSON Schema does not encode all invariants.
- Example test and benchmark provenance values are illustrative; sample formal models are supplied in [`../examples/greenfield/Inventory.tla`](../examples/greenfield/Inventory.tla), [`Inventory.cfg`](../examples/greenfield/Inventory.cfg), [`Inventory.als`](../examples/greenfield/Inventory.als) and [`metamodel.als`](metamodel.als).
- `VALID` means only that implemented checks found no errors; passing tests establish only the behavior they cover.
