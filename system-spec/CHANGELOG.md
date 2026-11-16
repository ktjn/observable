# Changelog

Toolkit versions (`system-spec --version`) are independent of a modeled system's `system.version`. Spec files stay valid across releases when changes only add optional fields; a validator older than a field rejects specs that use it.

Every entry ends with `Migrate:` — what an adopting project must do to upgrade (`none` when existing specs, commands and CI keep working unchanged; otherwise the concrete edits or commands, e.g. regenerate `schema.json`, rename a field, add a flag).

## Unreleased

## 0.16.0

- Added: `DOMAIN-AGENTS.md`, a domain-oriented subagent coordination methodology — accountable domain agents, coordinators for multi-domain scopes, schema/contract-first dependency ordering, and handoff/return contracts. Required within the optional development methodology whenever a `FEATURE.md`, `GREENFIELD.md` or `BROWNFIELD.md` scope spans more than one logical domain; a single-domain scope stays optional. Referenced from `FEATURE.md`, `GREENFIELD.md`, `BROWNFIELD.md`, `MODEL-SCOPE.md`, `WIRING.md` and `README.md`. Migrate: none for specs, commands and CI, which are unaffected; a project adopting the methodology for multi-domain work should assign a coordinator and a per-domain accountable agent going forward.

## 0.15.2

- Fixed: the `status` block rendered with `--root` names only what survives a squash or rebase merge, `no: changed since acceptance: core, spec`, instead of commit hashes and counts, so `--render-docs --check --root` passes on the main branch after a squash merge; the validation `note:` and `acceptance.stale` keep the hashes and counts, and `acceptance.stable` carries the committed wording. Migrate: none; re-render docs once (`--render-docs --root`) if a row already names hashes.

- Fixed: the git check of `system.acceptance` anchors the spec's changes on the newest commit that changed any acceptance field (compared as parsed YAML), not the last line matching `accepted_on`, and starts at the later of that commit and `revision`: a same-day re-acceptance is found, the accepted revision is never counted as a change since acceptance, and reformatting `accepted_on` no longer hides earlier spec changes. Migrate: none; a stale-acceptance note that counted the accepted revision or a re-acceptance commit disappears.

## 0.15.1

- Changed: `--render-docs` with `--root` in a git checkout renders the `status` block's `Acceptance current` row from the same git check as validation (`yes`, or `no: …` for an unknown or non-ancestor revision or changed components), so `--render-docs --check` fails once code or the spec changes after the accepted revision; without `--root` the row is unchanged (`yes (code not checked)`). Migrate: none without `--root`; a docs check that passes `--root` must re-render (`--render-docs`) when the acceptance goes stale, and the rendered row changes with each commit touching a component's `source_paths`.

- Fixed: a stage run with `--check-artifacts`, `--check-drift`, `--check-imports` or `--check-isolation` prints the same stage reports as a plain run (`note:` lines for acceptance, formal gaps, omitted categories, proposed and deferred scope) and `--json` carries `residual_risks`, `waivers`, `proposed`, `omitted_categories`, `formal_*`, `acceptance` and `deferred`; they were dropped. Migrate: none; a CI step that parses `--json` from a check run sees the extra keys, and a stale acceptance now shows in the same run that passes `--root`.

- Changed: `formal_no_evidence` (note and `--json`) lists only components with no passing `model-check`/`proof` record and no accepted risk named by `formal_assessment.risk`; the accepted gaps are reported apart as `formal_accepted_gaps` (component → risk ID) and a `note:` line. Migrate: none; a script reading `formal_no_evidence` now sees only the undecided gaps, so read `formal_accepted_gaps` too for the full list.

## 0.15.0

- Changed: the `tla`, `alloy`, `quint` and `lean` skills say to hash a normalized, reproducible result summary (TLC: version, verdict, distinct states, depth; Lean: build status and `#print axioms`) instead of the raw checker output, to keep the raw log out of the repository or unhashed, and to rerun fast configurations in CI comparing their summary with the committed one; simulation is reproducible only with a fixed seed and depth. Migrate: none; records that hash a raw log should hash the summary instead (`tlc.out` → `tlc.summary`) and drop the log.
- Added: with `--root` in a git checkout, the `system.acceptance` report names the components whose `source_paths` changed since `acceptance.revision` (`changed_since_acceptance`, also when there are no `subject_bindings`) and the spec file after the commit that wrote the acceptance, and reports an unknown or non-ancestor `revision`; `code_checked` says whether git was consulted and a note says code changes were not checked otherwise; an unquoted all-digit `revision` is refused with a message to quote it; the docs say which revision to record under squash or rebase merges. Migrate: none; the `status` block's `Acceptance current` cell reads `yes (code not checked)` when no code check ran, so re-render docs (`--render-docs`).
- Added: invariant `COMP-13` (at `release`): a component with `formal_assessment.applies: true` and no passing `model-check` or `proof` evidence fails unless the new `formal_assessment.risk` names a risk targeting it (or a capability it provides) with `treatment: acceptance` and `residual_accepted: true`; `COMP-12` also checks that a cited risk exists and is accepted. Migrate: for each component in the `formal_no_evidence` note, add the evidence, or add such a risk and set `formal_assessment.risk`; specs without an applicable assessment are unchanged (compatibility: the invariant is new at `release` for the `formal_assessment` field added in 0.14.0).
## 0.14.0

- Added: optional `system.acceptance` (`stage`, `accepted_by`, `accepted_on`, `revision`, `scope`, `notes`) records the acceptance decision apart from passing `--stage release`; every run reports it with whether the stage passes now (`acceptance` in `--json`, `note:` lines) and why it may be stale (evidence dated after `accepted_on`, `subject_bindings` at another revision), and the `--render-docs` `status` block shows it when recorded. Migrate: none; specs without it validate and render unchanged.
- Added: optional `Component.formal_assessment` (`applies`, `indicators`, `rationale`, `evidence`) and invariant `COMP-12` (at `verification`: indicators when it applies, a rationale when it does not, cited evidence exists and is `model-check` or `proof`); `--stage verification|release` lists components with no assessment (`formal_unassessed`) and applicable ones without passing `model-check` or `proof` evidence (`formal_no_evidence`) as notes and in `--json`; BROWNFIELD.md and its intake template gain the assessment step. Migrate: none; specs without it validate unchanged, and `--json` consumers see two new keys.
- Changed: an `--check-artifacts` hash mismatch on an evidence record's `artifacts` names the record (`... (evidence 'ev-id')`), and the `tla`, `alloy`, `quint` and `lean` skills all say to hash the model, its config or bounds file, the checker output (and for Lean `lean-toolchain`, `lake-manifest.json`) in `artifacts`, so editing a model behind a passing `model-check` or `proof` record is caught; VERIFIED-DEVELOPMENT.md plans a model-checker result adapter. Migrate: none; scripts that match the exact mismatch message must allow the added suffix, and existing model records gain `artifacts` by hashing the files (`sha256sum`).
- Added: `--evidence-report` rows carry `model_only` (true when the requirement's passing evidence is only `model-check` or `proof`, so no test ties the model to the code), and the `tla`, `alloy`, `quint` and `lean` skills each have a non-optional Correspondence step (trace replay, differential tests, refinement) recorded as `test` evidence. Migrate: none; the key is new in `--evidence-report` output, existing keys are unchanged.
- Added: skill [`lean`](.github/skills/lean/SKILL.md): Lean 4 proofs of a capability's invariants (`lake build`, `#print axioms`) recorded as `method: proof` Evidence. Migrate: none; copy the skill like the others (WIRING.md § Install skills).

## 0.13.2

- Fixed: `--bind` prints only `subject_bindings` as YAML content; the refreshed evidence digests follow as `#` comments (pasting the output whole used to fail on the non-spec `evidence_artifacts` key), a record's artifacts inherited from its `runs` entry are no longer repeated per record, and the run's own refreshed digests are listed once; `--json` has `evidence_artifacts` (own artifacts only) and `run_artifacts` (#247). Migrate: none; scripts that parsed `evidence_artifacts` from the YAML output should use `--json`.
- Fixed: with a root package exempted (`--ownership-ignore .`), `--check-ownership` no longer reports every `source_paths` entry outside a deeper package as a claim on an ignored package: an exempted root package covers only its own directory, so a plain directory such as `models/` can be owned, while claiming `.` itself is still refused (#248). Migrate: none; drop workarounds that moved such directories into a crate.
## 0.13.1

- Fixed: `--check-ownership` (with or without `--ownership-spec`) reports a `source_paths` entry that lies inside a package another component owns as overlapping ownership naming both components, and one inside a package `--ownership-ignore` exempts as a claim on an ignored package; before, only whole package directories were judged (#245). Migrate: none unless a spec has such an entry: move it to the package's owner, or drop it from the ignored package; a project-local tree check can be removed.
## 0.13.0

- Added: `--check-ownership --ownership-spec FILE` (repeatable) judges package ownership across the other specs of the same repository: a package is owned when a component of any given spec owns it, and claimed twice when components of two specs do; components are named `spec:id` in the messages, so partial specs pass without copying each other's paths into `--ownership-ignore` (#243). Migrate: none; replace `--ownership-ignore` globs that only listed another spec's packages with `--ownership-spec` for that spec.
## 0.12.1

- Changed: `--emit-test-map` writes `<file>::<path within the file>::<fn>` (the modules the file itself stands for are no longer repeated: `src/store.rs::tests::round_trips`, the form `--check-test-names` accepts); `verifies` also comes from evidence that cites the exact test or a `nextest::-p PKG` selector; each `--emit-test-map` pairs with its own `--cargo-metadata`, in order (one serves all), so a crate outside the workspace resolves too; paths are relative to `--root` (default: the current directory) instead of each metadata's `workspace_root` (#235). Migrate: regenerate maps made with 0.12.0, and evidence that cites `file::module::test` for a test in a non-root module becomes `file::test` (or `file::tests::test` for an inline module); pass `--root` when the CI step does not run at the repository root.
- Fixed: `--check-drift --check-ownership` and `--check-report --test-pattern`/`--test-glob` no longer read build output or ignored files: in a git work tree they use the tracked and not-ignored files, otherwise they skip hidden, `target*`, `node_modules` and venv directories (generated `Cargo.toml` files under `target/` were reported as unowned packages, and a crate's own `target/` was read for every run) (#236). Migrate: none; `--ownership-ignore` globs written for build output can be removed; a test file that is gitignored (or hidden, outside git) is no longer required to have reported tests.
- Changed: `--max-unbound N` counts only unbound tests owned by the model's internal components; `(unowned)` and external-component tests are still listed, marked as not counted, and `--json` adds `unbound_owned_tests` and `unbound_not_counted` (`unbound_tests` is still the total), so two specs checking one shared JUnit report each hold their own count (#238). Migrate: none unless a `--max-unbound` limit was set from the total; it can only go down, so lower it to `unbound_owned_tests`.
- Fixed: `--check-drift --check-ownership` counts `source_paths` only, as its message and flag text say; a package that appears only in some component's `test_paths` (a shared fixture crate) is reported as owned by no component, and the error names those test_paths (#237). Migrate: none; if a package was passing only through `test_paths`, add it to a component's `source_paths` or to `--ownership-ignore`.
## 0.12.0

- Fixed: the shipped `schema.json`, `fragment.schema.json` and `handoff-return.schema.json` use LF line endings (`.gitattributes` keeps them so), so their SHA-256 equals that of `system-spec --schema`, `--fragment-schema` and `--return-schema` output; a test compares the bytes; WIRING.md tells installed-package users to commit `system-spec --schema > schema.json` as the `$schema` target (#229). Migrate: none; a hash recorded from the old CRLF file changes once, re-record it (the parsed JSON is unchanged).
- Added: `--emit-test-map JUNIT --cargo-metadata FILE` turns cargo-nextest JUnit plus `cargo metadata --no-deps` output into a `--test-map` (each test's `<file>::<name>` by binary ID and `mod` path, `#[path]` followed; `verifies` from evidence citing the file; unresolved tests listed on stderr), and WIRING.md documents where nextest writes its JUnit and the shared `CARGO_TARGET_DIR` collision (#228). Migrate: none; Rust projects can replace their own test-to-file resolver with the generated map.
- Added: `--check-report --report-scope evidence` lists the unbound tests (named by no evidence) per owning component in the output and as `unbound_by_component` in `--json` (per file with `--verbose`, `unbound_files`), and `--max-unbound N` fails when more are unbound, naming them (#227). Migrate: none; `unbound_tests` and the note line are unchanged, per-component lines follow the note; CI can hold the current count with `--max-unbound`.
- Added: `--check-drift --check-ownership` (with `--ownership-ignore GLOB`, repeatable): a package from `--deps`, a Cargo.toml `[package]` or a named package.json under `--root` that belongs to no, or to several, internal components is an error naming the package and the components to add it to; off by default (#226). Migrate: none; to adopt, add the new `--check-ownership` flag to the CI drift step and list deliberate exceptions with `--ownership-ignore`.
- Added: `--check-report --root R --test-pattern REGEX` (repeatable): every file under a component's `test_paths` whose content matches the regex (per line) needs a reported test, for Rust, Go and other languages whose tests live in ordinary source files (#225). Migrate: none; Rust projects can pass `'^\s*#\[(?:\w+::)*(?:test|rstest)\b'` instead of `--test-glob '*.rs'`.

## 0.11.1

- Fixed: `--render-docs` recognises a block marker only when its comment stands alone on its line outside fenced code, so an inline mention or a fenced example no longer pairs with a later closing marker and deletes the text between (reported `VALID`); an unclosed block, a marker inside a block or a stray closing marker is an error naming the line and the file is left unchanged; CRLF files keep their line endings (#223). Migrate: none; if a document shows markers in inline code or a fence, they are now left as text.
## 0.11.0

- Added: with `--check-report --report-scope evidence` a test may back several requirements: bindings are optional, and any a cited test has (or a test under a cited file or selector) must include the record's `requirement@version`; `--test-map` `verifies` (and a map value) may be a list, and `--emit-evidence` yields one record per binding. The default strict scope still requires exactly one binding (#218). Migrate: none.
- Changed: `--render-docs` `contracts` lists each failure mode with its `interface.errors` entry (`card_declined → charge.declined`; a plain name when there is none) (#217). Migrate: none; re-run `--render-docs` to refresh rendered documents.
- Changed: `--check-test-names` also requires the module segments of `<file>::<module>::<name>` to be declared in the file, in order (`mod X` in `.rs`, `class X` in `.py`, otherwise as identifiers), so a wrong module path is caught (#215). Migrate: none, unless `--check-test-names` now reports a wrong module path in a `<file>::<module>::<name>` provenance; correct the module.
- Changed: releases: every `Unreleased` entry must end with `Migrate:` (the upgrade steps, or `none`); CI (`release_support.py --check-changelog`) and the `publish` plan reject entries without it. Migrate: none for adopting projects; contributors add `Migrate:` to their entries.

## 0.10.0

- Added: `--check-report` matches `test` evidence citing a test file (`tests/kv.rs`) or a runner selector in `test_paths` (`nextest::-p store`) to the reported tests under it: present when any matches, in agreement when all passed and any binding is the record's requirement; `--selector-prefix SELECTOR=PREFIX` maps other selectors to report-ID prefixes (nextest `-p NAME` built in); those tests count as named by evidence for `--report-scope evidence` (#211).
- Changed: `--render-docs` `requirements` shows each criterion as `id: text` (not the bare ID), and `risks` gains Description, Owner and Waives with its expiry; `report:risks` and `--report risks` keep the compact table (#210).
- Added: `--render-docs` kinds `layers` (declared order, internal components and their `source_paths`) and `domains` (description, `uses`, internal components) (#209).
## 0.9.0

- Changed: releases: `Unreleased` entries start with `Breaking:`, `Added:`, `Changed:` or `Fixed:` (CI checks it) and raise the suggested version above the public-surface check (`Added:` minor, `Breaking:` major); the `publish` plan is also printed in the job log and as an annotation, so a dry run shows its result.
- Added: `--render-docs` kinds: `requirements` gains Status (`specified (proposed)`) and Applies to; new `evidence` (one row per record: requirement, subject, criteria, method, provenance, run, verifier, result) and `status` (counts, open requirement IDs, runs, risks and waivers, bound subjects); `components` gains test requires, owned data and source paths; `capabilities` provider and auth; `data-models` derived from and schema (#202).
- Changed: Compatibility: `--check-artifacts`, `--check-isolation`, `--check-drift` and `--check-imports` without `--stage` or `--compat` run only the requested checks, whatever their result (before, `--check-artifacts` also ran the release stage when it passed); give `--stage` to run a stage too. A binding hash-mismatch message leaves out empty `stale requirements -; stale evidence -` parts (#201).
- Changed: `--check-imports`: a vendored `imports/<id>.yaml` is also used when `path` leaves `--root` (not only when it is missing), and every message names the file actually read, e.g. `sha256 mismatch for spec/imports/pay.yaml (vendored copy; declared path …)`, likewise for `--import-path` (#200).
- Added: `--test-map` entries may be `{verifies: requirement@version, test: <file>::<name>}`: the report ID is then treated as that `test` ID (and its file), so evidence matches runners whose JUnit IDs are `<binary-id>::<name>` (cargo-nextest) without rewriting the report; string entries unchanged. Not included: selector-aware provenance matching (#199).
- Added: `--render-docs`: a block may select its own spec, `<!-- system-spec: KIND spec=PATH -->` (relative to the Markdown file, loaded and structurally validated once per run), so one document can cover a spec and the spec importing it; blocks without `spec=` render as before (#198).
- Changed: Releases: `publish` has a `plan` job that suggests the version (major if a public invariant ID, CLI option or schema property was removed since the last tag, minor if added, else patch; `bump` overrides, `dry_run` previews), stops on an empty `Unreleased`, and a re-run resumes a pushed release (`release_support.py`); CI requires a changelog entry when `metamodel.py` or a schema changes.
- Changed: Releases run from one `publish` workflow dispatch (bump, changelog, tag, PyPI, GitHub release with digests); RELEASING.md.

## 0.8.0

Compatibility: all additions are optional (`Criterion.threshold`, `Evidence.measurements`, `runs`, `Evidence.run`); existing specs validate unchanged. Specs that use them need this validator.

- `--impact`: evidence of an `applies_to` requirement is invalidated only for subjects the change reaches (all of it when the requirement is the target); the requirement is still listed (#177).
- Optional `Criterion.threshold` (`metric`, `op`, `value`, `unit`) and `Evidence.measurements`; new `EV-09` (verification) checks `passed` agrees with the measurements and passing evidence records the metric; `--evidence-report` lists measured values (93 invariants). Regenerate `schema.json` and `fragment.schema.json` (#176).
- Optional `runs` (`Run`: `id`, `subject_revision`, `subject_version`, `artifacts`, `timestamp`, `verifier`, `environment`) and `Evidence.run`: a record inherits the fields it leaves unset from its run on load, replacing YAML anchors; unknown run is `REF-15` (92 invariants). Regenerate `schema.json` and `fragment.schema.json`. Not included: per-run `--evidence-report` and `--emit-evidence` grouping (#175).
- SHA-256 fields (`subject_bindings`/evidence/baseline `artifacts`, `imports[].sha256`, view and experience reference `sha256`) reject a YAML number with "SHA-256 must be a quoted string (YAML read it as a number)"; no coercion, since leading zeros are lost or read as octal (#174).
- `--check-imports --import-path ID=FILE` (repeatable) and a vendored `imports/<id>.yaml` next to the spec (used when `path` is missing) supply an imported spec without a sibling checkout; the pin's `sha256` verifies it (#173).
- `--impact` and `--plan` accept imported capability IDs (`<import>:<capability>`), seeded with the components that `requires` or `test_requires` them (#172).
- `--check-artifacts --check-test-names`: a `<file>::<name>` test provenance must name a test found in the file (textual identifier match, opt-in) (#171).
- `--bind [SUBJECT…] --revision REV --root R [--include GLOB…]` prints `subject_bindings` (and refreshed evidence `artifacts` digests) from the provider's tracked source files, refusing when they differ from `REV`; BROWNFIELD.md § Keeping a bound spec current (#170).
- `--check-artifacts`: a `subject_bindings` hash mismatch names the subject, the requirements targeting it and the evidence bound to it that are stale, in the message and as `stale` with `--json` (#169).
- `--impact-paths PATH…` (`-` reads stdin): impact of changed files, mapped to components through `source_paths`/`test_paths`; `--impact` output gains `tests` (test paths and selectors of affected components), `--impact-paths` adds `changed_components` and `unmapped_paths` (#168).
- `--render-docs FILE… [--check]` regenerates marker-delimited Markdown blocks (element tables, `report:*`, `diagram:*`) from the model with GitHub-safe cells; `--check` fails on stale files (#167). `--report risks` cells are escaped too.
- `--check-drift --deps FILE`: language-neutral dependency graph (packages with paths, `normal|dev|build` edges) checked like source imports against `requires`, `bindings` and `test_requires`; unowned packages are `unchecked` (#166).
- `--check-report --report-scope evidence` checks only tests named by `test` evidence and reports the rest as a count (brownfield adoption); `--test-map FILE` binds tests to `requirement@version` for `--check-report` and `--emit-evidence`, for runners without per-test properties (#165).
- `--report` honours `--json` (coverage: one object per breakdown keyed by dimension; risks: row objects); coverage tables get `### By <dimension>` headings; priority and mandatory breakdowns with a single value are omitted (#164).
- `--diagram components`: an external component's `API:` label omits capabilities reached only through `test_requires` (#163).
- `via` (`REF-05`) accepts version-constrained references (`base:txn@^1.0`) on either side, resolved like `requires`; the constraint is ignored for the mapping (#162).
- `--diagram hierarchy` and `--diagram risks` render left-to-right (`graph LR`): siblings stack vertically, so large specs stay readable instead of very wide; `components` and `runtime` stay top-to-bottom (#158).
- `--check-report --root R --test-glob PATTERN`: every file matching a project-supplied pattern under a component's `test_paths` needs at least one reported test (file from the `file` property/attribute or the `<file>::` prefix of the `test` property); runner-independent, opt-in.

## 0.7.0

Compatibility: all additions are optional (`Interface.publish`, `Requirement.applies_to`); existing specs validate unchanged. Specs that use them need this validator.

- `--diagram runtime`: flow view (components boxed by domain inside the system, externals outside; edges labelled with `interface.operation`, wrapped and merged when parallel; caller → provider, `async` dashed; `via` drawn through the mediator, `via` on a `publish: true` interface drawn publisher → mediator → subscriber; no layers or data). All `--diagram` kinds render top-to-bottom (tested). `--help` and README list every kind with its default (#150).
- `Interface.publish` (optional, default `false`): the provider publishes events that consumers subscribe to; existing specs validate unchanged.
- Feedback must not leak project specifics: README § Giving feedback, `CONSTITUTION.md` self-improvement step 4, agent instructions (`AGENTS.md`, `WIRING.md`) and an issue template with a checkbox (#152).
- System-wide requirements (#135 follow-up): optional `Requirement.applies_to` (`Selector`: `kind`, `has_interface`, `domain`, `layer`, `external`) on a system-targeted requirement; each selected entity needs passing evidence naming it as `subject_id`; `--evidence-report` lists selected and uncovered entities; `REQ-11` (inventory). Existing specs validate unchanged; specs using it need this validator.
- CI: package job runs on Ubuntu with the latest Python only; GitHub Actions updated to Node 24 versions; Dependabot added for Actions and uv lockfiles. `PKG-09` v1.1 drops Windows and 3.12.

## 0.6.0

Compatibility: all additions are optional or opt-in (`Criterion`, `holder`, `evidence_policy`, `evidence_methods`, `release_scope`, `SpecImport.mode: baseline`); existing specs validate unchanged and `SpecImport.capabilities` now defaults to `{}`. New invariants apply only when their feature is used (`REQ-10`, `EV-07`, `EV-08`). Specs that use them need this validator.

- `--evidence-report` (#113): per active requirement version, the validator's own verification state (verified, failing/uncovered criteria, blanket and skipped records, methods, holder, priority). Opt-in `system.evidence_policy.require_criteria`: records without `criteria` are an error at verification (`EV-08`) and no longer cover the whole requirement.
- Test-only dependencies (#127): optional `Component.test_requires` resolves like `requires` (`COMP-04`) and satisfies `--check-drift` for integration-scoped test edges, but is exempt from `COMP-05`, `COMP-06`, `DOM-02`, context slices and fakes; `COMP-11` forbids it on external components.
- Release scope (#114): `system.release_scope` or `--release-scope P0[,P1]` limits release obligations (`STAT-04`, `REQ-04`–`REQ-06`, `REQ-08`, `RISK-04`, `EXP-07`) to in-scope priorities; out-of-scope requirements, their target entities, the components providing those and risks only they mitigate are reported `deferred`, never passed.
- Obligation holder (#115): optional `Requirement.holder`; `system.evidence_methods` maps holders to allowed evidence methods (`EV-07`).
- Criterion IDs (#116): a criterion may be `{id, text}`; evidence and handoff results name it by ID or text; IDs unique per requirement (`REQ-10`).
- Baseline imports (#117): `SpecImport.mode: baseline` with `artifacts`, `allow_fields`, `id_prefix` and `refinements` (`path`, `reason`, `decision`); `--check-imports` reports undeclared changes or removals of baseline requirements, risks, views, experiences and state sets, and stale refinements.
- `--diagram`: layer boxes are dashed (`style l_<layer> stroke-dasharray:6 4`), so they are distinguishable from the solid domain boxes nested inside them.

Compatibility: `EXP-09` to `EXP-12` are new verification-stage invariants for `critical` experiences (`EXP-09`/`EXP-10` only when also `exhaustive`); specs with experiences can need added interactions or scenarios. Commands combining `--check-*` with an explicit `--stage` or `--compat` now also run the stage and can fail where they printed `VALID` before.

- Fixed: `--check-drift`, `--check-imports` and `--check-isolation` (and `--check-artifacts` on failure) silently dropped an explicit `--stage`/`--compat`. They now run together and report all errors.
- Experience feedback from the Observable 2.0 adoption ([record](archive/feedback/observable-2.0-adoption.md)): `EXP-09` (view actions trigger interactions), `EXP-10` (view-specific states are reached), `EXP-11` (each failure mode's state is reached by a negative scenario of that interaction), `EXP-12` (negative scenarios exercise one interaction; scenario states lie on their interactions' views). Example `examples/experience/` updated.
- Docs: GREENFIELD.md § Planned components and § Adopting an upstream specification; FEATURE.md rules on gating spec-only changes and building on the validator's evidence evaluation; EXPERIENCE.md scenario guidance and the limit of structural checks.
- Distributed specs: `SpecImport.link` (owning repository URL, no revision), `Workspace`/`Member` manifest (`Member.stage` per-member override) and `--workspace [--stage]` cross-member checks (member validation, import pins, link agreement, unique system IDs, acyclic imports); `--check-imports` hints `(clone <link>)` for a missing checkout. [WORKSPACE.md](WORKSPACE.md).
- Hierarchical specs (#124): `Component.realized_by: <import-id>` (black-box subsystem realized by a child spec), optional `system.exports` (exported capability surface; default all), cross-level requirement links (`<import-id>:<requirement-id>` in `depends_on` and `refines`), and `--check-imports` boundary verification (contract inclusion `child.provides ⊇ component.provides`, `child.external_requires ⊆ component.requires`, capability/data compatibility, cross-spec requirement verification, and child release maturity for verified in-scope components).
- Docs: GREENFIELD.md § First iteration.
- Docs: [MODERNIZATION.md](MODERNIZATION.md) track and [mechanism decision template](templates/MODERNIZATION-DECISION.md); [`ddd`](.github/skills/ddd/SKILL.md) skill; test-first unit tests bind to component sub-requirements (CONSTITUTION.md § Traceability, FEATURE.md). Feedback: [record](archive/feedback/modernization-feedback.md).
- Proposals: #113 evidence report and criterion-coverage policy, #114 release scope by priority, #115 obligation holder, #116 criterion IDs, #117 baseline import mode.

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

- `--diagram` (all kinds) renders top-to-bottom (`graph TB`) instead of left-to-right, so large diagrams stay readable instead of shrinking to the page width (#139).

- `Requirement.proposed` (#135): optional flag for statements not yet agreed; passes the stages as `specified`, reported as `proposed` in the validator output, deferred (never passed) at release.

- `--report coverage` counts requirements per owning component (capabilities and data models roll up to their provider or owner) with a total row; `--diagram hierarchy` shows component totals, groups external components and styles capabilities, data models and externals (#135 comment).

- Adoption feedback (#135): `--diagram hierarchy|risks`, `--report risks|coverage`; `--diagram` nodes sorted by ID (stable across fragment layout); `REF-02` ambiguity names the colliding kinds; GREENFIELD.md § Greenfield with existing design documents (extract requirements, fragment-per-target, `external` components, docs-only target `verification`); spec.md fragment convention. 
