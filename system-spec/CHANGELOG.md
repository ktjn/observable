# Changelog

Toolkit versions (`system-spec --version`) are independent of a modeled system's `system.version`. Spec files stay valid across releases when changes only add optional fields; a validator older than a field rejects specs that use it.

## Unreleased

## 0.5.0

Compatibility: component isolation is opt-in (`standalone_harness`, `provided_mocks`, `--check-isolation`); existing specs validate unchanged. `--check-isolation` executes project code from `--root`; run it only on trusted repositories.

- Component isolation ([`COMPONENT-ISOLATION.md`](COMPONENT-ISOLATION.md)): `Component.standalone_harness` and `Component.provided_mocks` (`MockBinding`: `path`, `contract_suite`, `scenarios`); `--check-isolation --root` runs each harness with dependency fakes and each provided capability's contract suite against the real provider and its programmable mock. Example: [`examples/components/`](examples/components/README.md).
- Docs: [`REPLACEMENT.md`](REPLACEMENT.md) (implementation replacement); greenfield/brownfield convergence clarified.

Compatibility: additions to the optional experience extension; existing experience specs validate unchanged. Specs that use the new fields need this validator.

- Experience feedback from the observability-spec pilot (#99 to #106): `Scenario.requirements` (several requirements, all must pass); `Experience.entries` and `Interaction.from_entry` (starts that are not views); `Spec.state_sets` and `View.state_sets` (shared view states); `Constraint` (`value`, `requirement`) in `Experience.constraints`; `VisualReference.consistent_with`; `Experience.exhaustive` and `not_exercised` with new `EXP-08`. `EXP-02` and `EXP-07` cover the new references. Documented in-view actions as self-transitions and the split between failure modes and invariants.
Compatibility: the experience extension is optional; specs without `views` and `experiences` validate unchanged. Specs that use it need this validator.

- Optional experience specification ([`EXPERIENCE.md`](EXPERIENCE.md), `spec.md` § 13): `Spec.views` and `Spec.experiences` (interactions, view-scoped states, scenarios, visual references with illustrative/normative authority). Invariants `EXP-01` to `EXP-07`, applied only to declared experiences; `--check-artifacts` checks reference files. Example: [`examples/experience/`](examples/experience/README.md). Addresses issue #97 (adoption steps 1-3).
Compatibility: all additions are optional; existing specs validate unchanged. Specs that use the new fields need this validator (older ones reject unknown fields). `REQ-09` applies only where `superseded_by` is set.

- `Requirement.statement`, `priority`, `owner` and `superseded_by`; `description` on `Capability`, `Component` and `Risk`; `owner` on `Component` and `Risk`. `priority` has no effect on invariants and is listed by `--plan`. `REQ-09` (inventory): `superseded_by` is set only on deprecated requirements and names an active requirement.
- `system.verification_categories` narrows `REQ-01` to a project profile; omitted categories are reported (`omitted_categories` with `--json`, a `note:` line otherwise).
- Docs: `refines` means co-obligation (`REQ-08`); use `depends_on` across priority or phase.
- Example: [`examples/business/`](examples/business/README.md), a business-level specification validated at `verification`.
- Docs: [`FEATURE.md`](FEATURE.md) § Decompose and hand off states the requirement-first order, sub-requirement split per component, wave-based parallelism, handoff bundle and join in one place; no behavior change.
- Docs: [`FEATURE.md`](FEATURE.md) § Using with OpenSpec maps an OpenSpec change workflow onto the feature steps; no integration is provided.
- Skill [`quint`](.github/skills/quint/SKILL.md): Quint models (typecheck, `run` simulation as `analysis`, `verify` bounded check as `model-check`) recorded as Evidence.
Compatibility: `REQ-08` is a new release-stage invariant. Specs where a mandatory or mitigating requirement is refined (`refines`) by a requirement without passing evidence now fail at `release`; the new flags are opt-in.

- `REQ-08` (release): obligation passes to refining requirements (sub-requirements), and obligated or mitigating requirements are verified only when every sub-requirement is. A parent still needs its own (integration) evidence.
- `--check-report JUNIT` / `--emit-evidence JUNIT`: tests bind exactly one requirement version through a `verifies` JUnit property; `test` evidence must agree with the run reports and can be derived from them. `reference-implementation/conftest.py` shows a pytest `verifies` marker.
- Docs: [`MODEL-SCOPE.md`](MODEL-SCOPE.md) states that the metamodel is conceptual and platform independent; methodology and the reference validator are optional, and acceptance policy belongs to the consuming project.

## 0.4.0

Compatibility: all additions are optional or opt-in (`imports`, `include`, `domain`, `external`, `via`, `test_scope`, `Evidence.criteria`); existing specs validate unchanged. Specs that use them need a 0.4.0+ validator (older ones reject unknown fields). New invariants apply only when their feature is used (`IMP-01`, `EV-06`, `COMP-11`, `DOM-*`).

- Fixed: `system-spec export` no longer copies `__pycache__`/`.pyc` files that pip generates inside the installed toolkit.
- `--check-drift` now lists components without `source_paths` as `unchecked` instead of passing them silently.
- `--export backstage|structurizr`: Backstage catalog entities and a Structurizr DSL workspace from the spec (`--owner` for Backstage).
- Cross-spec references (`MKT-FB-01`): `system.imports` (`SpecImport`) pins another spec; `<import>:<capability>` is provided by external components and required by consumers without copying contracts; `IMP-01` (inventory); `--check-imports --root` verifies pins against the imported spec. Specs using `imports` need a validator that includes it.
- `Evidence.criteria` and `EV-06` (inventory): evidence can cover individual requirement criteria; a requirement is verified when every criterion is covered by passing evidence, and `REQ-04`/`REQ-05` name failing or uncovered criteria (`SCL-FB-05`). Records without criteria behave as before. Handoff return results accept `criteria` the same way.
- Handoff return: `HandoffReturn` model, `handoff-return.schema.json`, `--return-schema` and `--check-return FILE`: structured agent results (requirement-linked results, unmet with reasons, changed paths, findings) checked against the spec and component scope.
- `--check-drift` scans JS/TS imports (`import`/`export from`/`require`/dynamic `import()`), following relative specifiers and workspace package names from `package.json`; same rules as Python (declared dependency, tests use fakes).
- `system.domains` (`id`, `description`, `uses`) and `Component.domain` (optional): domains are orthogonal to layers (n:m). `DOM-01` declared, `DOM-02` cross-domain dependencies in `uses` (waiver `domain-violation`), `DOM-03` acyclic `uses`. `--diagram` nests domains inside layers. Addresses `SCL-FB-01`, `SCL-FB-02`.
- `Component.external` and `COMP-11` (architecture): providers outside the system (platforms, third-party APIs) declare contracts only; exempt from layers, domains, budgets and tests; drift targets whose own sources are not scanned; `--diagram` draws them as hexagons outside the system. Addresses `MKT-FB-01`.
- `Component.via` (`REF-05`): runtime-mediated dependencies name their mediating capability; listed as `unchecked` by `--check-drift`, dashed in `--diagram`. Addresses `MKT-FB-02`.
- `Component.test_scope: integration`: exempts assembly/end-to-end tests from `COMP-09` and lets them depend on providers of required capabilities in `--check-drift`; components without `source_paths` are listed as `unchecked`. Addresses `MKT-FB-03`.
- Split specs: `include` patterns in the root spec merge fragment files (any section except `system`); `fragment.schema.json` and `--fragment-schema` for editors. New optional `include` key on the root; `examples/components/` is now split. Specs using `include` need a validator that includes it.

## 0.3.0

Compatibility: all additions are optional or opt-in (`test_paths`, `fakes`, `Evidence.environment`); existing specs validate unchanged. Specs that use these fields need a 0.3.0+ validator (older ones reject unknown fields). Declaring `fakes` or `test_paths` activates `COMP-08` to `COMP-10`.

- `COMP-10` (release): a capability that a consumer fakes needs a verified `contract` requirement at its provider. Existing specs unaffected unless they declare `fakes`.
- `Evidence.environment` (optional): host/OS/CI job, distinguishing local from remote runs (`SCL-FB-06`).
- `--plan <id>`: provider-first delivery waves for a change, with per-component bindings and evidence to revisit.
- `--check-scope <component> --changed <file|->`: fails when a change touches paths outside the component's source/test/fake/schema paths (handoff scope guard).
- `test_paths`/`fakes` accept `tool::selector` entries for package-level runners (e.g. `nextest::-p crate`); `COMP-08`/`COMP-09` match them by prefix. `--check-drift` also reads `Cargo.toml` path/workspace dependencies. Addresses `SCL-FB-03`, `SCL-FB-04`.
- New [`FEATURE.md`](FEATURE.md) track: changing an already-specified system.
- `Component.test_paths` and `COMP-08` (verification): test evidence for a component that declares `test_paths` must come from files under them. Opt-in; existing specs unaffected. Specs using `test_paths` need a validator that includes it.
- `Component.fakes` and `COMP-09` (verification): a component with `test_paths` needs a fake under them for each required capability.
- `--check-drift` also scans `test_paths` and rejects tests importing other components (use the fakes); `--check-artifacts` verifies `test_paths` and `fakes` exist.
- `--diagram`: data models sit in their owner's layer; persisted ones (`namespace`/`storage_role`) are cylinders, logical-only ones dashed boxes; node IDs are prefixed per kind (`s_`, `l_`, `c_`, `d_`), so a component named like its layer renders.
- New [`examples/components/`](examples/components/README.md): standalone components with their own tests.

## 0.2.0

- `--check-drift --root`: Python imports between components vs `requires`/`bindings`; JSON Schema properties vs data model fields.
- `WIRING.md`: uv install, schema modeline, CI gate, path anchoring, skill link rewrite, adoption guidance.

## 0.1.0

- First release: validator CLI (`system-spec`), JSON Schema, offline `export`, workflows, examples and skills.
