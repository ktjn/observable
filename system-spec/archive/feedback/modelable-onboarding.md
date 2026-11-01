# Modelable onboarding feedback

Date: 2026-10-31. The findings below describe the original pinned baseline. Implementation dispositions follow; historical reproductions are retained.

Observed baselines:

- Modelable `0fead2321bdef11c72b2746eb57f3a8b0e327c94` (1.15.1).
- System-specification `00188b037797fd37240dab9a0a11c431815e3ac9`.
- Intake lives in Modelable at `docs/superpowers/specs/system-spec/`: product inventory, bounded compilation specification, vendored schema, source/environment hashes, characterization output and gate record.
- Local characterization: 221 passed, 2 skipped. Both skips require directory symlink support unavailable on this Windows host. No whole-product or release acceptance is claimed.

## Findings and proposed changes

| ID | Observed fact / gap | Proposed change and acceptance |
|---|---|---|
| MOD-FB-01 | Modelable already has authoritative `docs/architecture.md`, `modelable capabilities` and native gates. Its `modelable spec` command tracks dbt/FHIR/ODCS imports, not this metamodel. | Add an onboarding authority/tool map to the brownfield template: existing authorities, imported catalogs, system-spec document location, validator/schema revision and native gates. Test examples without implying that capability status or an imported schema is acceptance evidence. |
| MOD-FB-02 | The six product inventory boundaries are not fully characterized. A compilation slice can advance while browser/native/integration mapping remains open. The model has no explicit adoption scope or external component record. | Add a documented scope/dependency register first; distinguish whole-product, slice and external/deferred boundaries in gate records. Consider a later schema extension for external providers and excluded boundaries; missing consumers must never silently become resolved internal dependencies. |
| MOD-FB-03 | `DataModel.classification` defaults to `public` and has no unknown/provisional value. Brownfield discovery cannot safely infer classification for source previews, paths and user workspaces. | Permit explicit unknown/provisional classification at inventory, with a named review obligation and conservative handling policy. Require a reviewed value before accepting architecture/release; test propagation with unresolved sources. The Modelable slice uses `restricted` as a declared intake handling policy, not observed product metadata. |
| MOD-FB-04 | Evidence documentation permits an immutable source revision, but release validation compares `subject_revision` to the modeled subject's numeric `version`. The actual source commit differs from contract version `1.0`. | Separate subject contract version from implementation/source revision and bind both to the reviewed artifact. Do not merely accept arbitrary non-empty hashes: verify the binding or retain an explicit evidence-review gap. Regression: the actual source hash with a matching contract/source binding passes; stale subject version, stale source revision or missing binding fails. |
| MOD-FB-05 | Compilation is a multi-step local API: direct, preview, confirm/apply and discard. Direct and conversational paths differ; a post-commit cleanup exception means files already changed. Single `Interface.operation`, boolean `idempotent` and failure-name mappings cannot fully describe this protocol. | Document local-operation state machines, confirmation binding and committed/uncommitted outcomes in the handoff. Assess a later protocol/state-transition representation rather than inventing HTTP codes or treating every exception as rollback. Characterize stale preview, exact staged promotion, lock freshness, rollback and committed cleanup separately. |
| MOD-FB-06 | Native source/test/dependency/schema hashes and skipped-test details need sidecar records. The validator only parses `source_paths`/`schema_path`; it cannot verify closure or current execution artifacts. A context slice omits evidence and no tool resolves source paths against a declared root. | Define a repository-root and artifact/evidence binding convention, pin validator/schema versions, retain hashed execution metadata and report skips explicitly. Add optional checks for path resolution, source/schema drift and evidence artifacts without replacing native gates or weakening requirements. |

## Reproductions and evidence

- `cli/src/modelable/specs/tracking.py` declares `SUPPORTED_SPEC_KINDS = {"dbt", "fhir", "odcs"}` and stores `.modelable/specs.yml`.
- `cli/src/modelable/operations/compilation.py` defines the five captured top-level DTOs and `CompilationService`; `file_transaction.py` defines `FileTransactionCommittedError.committed = True`. The captured DTO field-name sets were compared against the actual source AST.
- `test_compilation_service.py::test_apply_rejects_changed_resolved_parent_even_with_same_bytes` and `::test_preview_rejects_symlink_that_escapes_workspace` skip with `directory symlinks are unavailable` (source lines 422 and 1136 at this revision). Separate skip diagnostics are retained with the onboarding evidence.
- [`metamodel.py`](../../reference-implementation/metamodel.py) accepts the intake at `inventory` and compilation slice at `verification`; the latter has one declared `verification-category` waiver expiring after verification. No waiver is removed and no metadata check is bypassed.
- Calling `validate_spec(compilation_spec, 'release')` produces eight `[EV-04]` errors for actual source-hash evidence, including: `Evidence 'ev-mod-comp-01' requires timestamp, verifier and subject_revision matching 'direct-compilation' version`. Timestamp and verifier are present; the source hash is not the subject version. Other release failures remain legitimate because status/coverage/remediation work is unfinished.

Reproduce from `reference-implementation/` with the repositories as siblings:

```text
python metamodel.py ../../modelable/docs/superpowers/specs/system-spec/inventory.yaml --stage inventory --json
python metamodel.py ../../modelable/docs/superpowers/specs/system-spec/compilation.yaml --stage verification --json
python metamodel.py ../../modelable/docs/superpowers/specs/system-spec/compilation.yaml --stage release --json
```

The final command is expected to fail; it is diagnostic evidence, not an acceptance command that should be made green by changing recorded revisions.

## Implementation disposition

| Finding | Implemented response |
|---|---|
| MOD-FB-01 | [Brownfield intake template](../../templates/BROWNFIELD-INTAKE.md) and constitution gate record now require existing authorities, command/tool distinctions, validator/schema pins and native gates. |
| MOD-FB-02 | Whole-product versus slice scope and external/deferred/unknown boundaries are explicit intake/gate records. Unresolved providers keep architecture open; external-provider resolution is deliberately not invented as new schema behavior. |
| MOD-FB-03 | Classification defaults to `unknown`; inventory requires an active `classification_review` requirement (`DATA-06`), architecture requires known classifications (`DATA-05`), and unresolved lineage cannot be bypassed by declassification. |
| MOD-FB-04 | `Spec.subject_bindings` records contract version, reviewed source revision and source hashes. Evidence can carry distinct `subject_version` / `subject_revision`; release rejects stale or absent bindings (`EV-04`, `REF-14`). The legacy version-valued format remains supported without a binding. |
| MOD-FB-05 | The intake template and implementation handoff require local operation transitions, confirmation/freshness, durable writes, rollback and committed-with-cleanup-error recovery. The protocol remains a review record; no executable state-machine validation is claimed. |
| MOD-FB-06 | Source and evidence file hash bindings, explicit `skipped` checks (`EV-05`), and optional `--check-artifacts --root` integrity checks are implemented. Context retains applicable source bindings. Git authenticity, build correspondence and semantic schema drift remain independent review/native-gate obligations. |

- New behavior has regression tests in [`test_onboarding.py`](../../reference-implementation/test_onboarding.py); new invariants have invalid corpus cases and the generated schema is updated.
- Migration: existing specifications omitting classification must state a reviewed value or inventory review obligation. Existing source-hash evidence must add explicit contract/source bindings; arbitrary hashes alone still fail release. Modelable's historical onboarding remains pinned to its original validator/schema and is not silently upgraded by this PR.
- Suggested future external-provider and executable-protocol schema extensions remain deferred design options. The requested workflow conventions are implemented without expanding the reference format into a deployment topology or protocol engine.

## Remaining adoption work

- Review these changes against Modelable's pinned intake before migrating that intake to the new schema and source-binding format.
- Preserve compilation's five owned top-level DTOs under one promotion boundary using the existing `Component.context_budget` override. Do not split the transaction just to satisfy the default four-model budget.
- Expand Modelable's dependency/ownership and nested-type map, run missing symlink coverage on a suitable host, establish measured quality requirements and review formal applicability before higher conformance claims.
- These notes retain the reporting and disposition trail required by [constitution self-improvement](../../CONSTITUTION.md#self-improvement-and-model-feedback); implementation does not close Modelable's open conformance gates.

## Evidence migration follow-up (2026-10-31)

- Scope: Modelable compilation at `8de5b55bf07cff4a619c6ae25f2c3368290d310b` (1.15.2), using validator/schema `b187e43628c7dc89935d17c2c382094ad37eb2eb`. The original inventory, schema, run and decision record remain historical; migration does not advance whole-product conformance.
- Reproduction: the old provenance string `evidence/characterization.txt; cli/tests/test_compilation_service.py::...` fails artifact checking. It combines two references and treats a nested spec-relative log path as repository-relative. Old release diagnostics also contain eight EV-04 version/source mismatches.
- Migration: use one root-relative provenance file plus a selector; hash execution logs, environment and tests separately. Bind each capability's contract version `1.0` to the reviewed source revision and three operation-file hashes. Six test files and those three source files match the original capture; dependencies and environment are recaptured.
- Local results: 221 passed, 2 skipped. Inventory and compilation verification pass with artifact checks and the existing category waiver; original capability/data compatibility passes. Five negative probes reject stale source/contract versions, a missing binding and mismatched source/log hashes. Release no longer reports EV-04 mismatches.
- Skips remain explicit: MOD-COMP-10 has a partial record naming both unexecuted directory-symlink cases and remains unverified. The eight passing records cover separate executed tests. EV-05 and the existing category/status/remediation obligations still block release.
- Disposition: document repository-root path resolution, one-file provenance, separate artifact maps, historical preservation and criterion-scoped skips in the brownfield guide/template. No validator behavior change is required. Artifact checks do not validate selectors, full dependency correspondence, Git authenticity or acceptance.
- Remaining work: transitive type/ownership/side-effect mapping, classification review, suitable-host symlink coverage, measured quality requirements and formal applicability. Maintainers retain acceptance ownership.

### Native-gate discovery: requested graph state

- The migration's required native gate exposed a pre-existing dense-graph node/control overlap. The unchanged browser test failed once in eight repeated runs; waiting for the actual Entity layout made the same no-overlap assertion fail in all eight runs. A 250 ms diagnostic sample retained the overlap.
- The old readiness checks could inspect nodes left from the previous graph mode, so a passing run did not necessarily exercise the requested layout. Characterization must establish that the requested state is rendered before asserting its geometry or other output. Preserve the original assertion and report product rework separately from a test synchronization fix.
- Repair: move navigation controls into a separate row above the canvas while preserving zoom/fit behavior and the existing zoom/no-overlap assertions. The test now waits for the requested layout, then checks zoom and fit interactions. The repaired test passed eight repetitions with eight workers; nine related graph/responsive checks and fifteen targeted graph unit tests also passed. A captured browser frame confirms controls remain outside the graph surface.
- Modelable publication still requires the full native gate on the repair commit; targeted results do not replace it. This discovery does not invalidate the separate Python compilation characterization or close whole-product browser acceptance.

## Dependency and suitable-host follow-up (2026-10-31)

- Modelable discovery at merged revision `3379c7c633f8b2690a4668cdb57abb5a07b9029d` records 141 internal source modules and nineteen declared dataclass shapes, plus providers, state ownership and direct/preview/apply/discard transitions. The original migration plan is archived; the ongoing onboarding specification remains active.
- Import reachability is distinct from runtime/provider topology. `AffectedDefinition` is imported from the workspace editor as a payload definition, not an LLM-provider call. OCI `push`/`pull` deliberately raise unsupported-operation errors; preview rejects OCI and direct execution surfaces that error. A named adapter is not evidence of a working remote integration.
- State review preserves the staging/promotion consistency boundary. Direct and conversational paths differ in lock/guard/overlap policy. Helper contention/filesystem exceptions need public error/adapter review; multiple file replacements with compensation do not establish crash atomicity. These remain explicit MOD-COMP-09 rework, not silently corrected contracts or passing conformance claims.
- Both previously skipped directory-symlink cases passed on Linux: two passed, zero skipped, against the original runtime source `8de5b55bf07cff4a619c6ae25f2c3368290d310b`. Retained output, JUnit, Python/locked dependency environment, image digest/ID and source hashes bind the result. New evidence supersedes only the historical MOD-COMP-10 partial record; Windows skip output and platform limits remain visible.
- Provenance discovery: the first `git archive` inherited Windows `core.autocrlf=true` and exported CRLF, contrary to the capture's assumed LF description. The active capture was rebuilt with explicit `core.autocrlf=false` / `core.eol=lf`, implementation/test bytes were checked against Git blobs, and the two tests passed again. No assertions, source tests or Windows privileges were changed.
- Disposition: extend the intake template with import/provider/ownership distinctions, actual export-byte verification, platform-scoped supersession and operation-variant/error/crash review. No metamodel/schema extension is needed for these review records. Quality categories, classification, shared IR/schema semantics, public failure review, formal assessment and whole-product release remain open.

## Error-boundary follow-up (2026-10-31)

- Source review distinguishes the Python compilation service, CLI, shared filesystem conversation, LSP serialization/request boundary and VS Code participant. The service's raw lock/IO families exceed the short frozen failure catalog; the model remains in rework rather than being silently relabeled complete.
- Five CLI regression cases reproduced empty user-facing output for transaction, workspace-lock, ledger-lock, filesystem and committed-cleanup failures. A CLI-only repair translates known operational failures into the existing Click error format, preserves nonzero status, and includes committed written paths and cleanup detail. Unexpected programming faults still propagate; Python service exception types and existing rich diagnostics are unchanged.
- Focused characterization after the repair passed 295 tests with six explicit platform skips, including existing committed-conversation behavior and real JSON-RPC happy-path coverage. The CLI subset passed 69 tests. These results do not establish every transport fault mapping or manual editor/model acceptance.
- Outcome is not surface status: a committed-cleanup exception remains a nonzero CLI result, while the shared conversation returns `kind: applied` with writes/audit/cleanup warnings. Ordinary apply failures may retain cleanup state or clear pending state depending on staging existence; VS Code cache retention alone does not prove retry eligibility. These distinctions require explicit recovery review.
- Compatibility probe: adding discovered operational failures to the frozen version reports `COMPAT-04` and `COMPAT-08`; a major bump removes only the version diagnostic, while `COMPAT-04` remains. The probe does not modify the specification. Consumer impact and baseline-correction disposition remain required; version changes are not a bypass for incompatibility.
- Disposition: extend the intake template with adapter-specific outcome/status/pending/retry records and a distinction between pre-existing model discrepancies and new implementation behavior. No metamodel/schema change is needed. CLI presentation can be remediated independently while the Python failure catalog, failure-path transport coverage and broader MOD-COMP-09 obligations remain open.

## Transport and successful-promotion cleanup follow-up (2026-10-31)

- Real stdio JSON-RPC characterization exposed a second cleanup path: when promotion returns normally but final staging removal fails, direct/apply raised ordinary `CompilationError`; conversational apply returned `kind: error` despite committed writes. Prior committed-cleanup coverage injected an already-committed transaction error, so it did not cover this path.
- Regression-first evidence: both direct/apply service cases failed for the incorrect exception type; the transport case failed because the response was `error` rather than `applied`. A shared successful-promotion cleanup helper now preserves the existing `FileTransactionCommittedError`, written paths and original cleanup cause. No frozen failure modes or data fields are added.
- Eight new tests pass, including real protocol guards and filesystem/retry assertions. Unsupported protocol version, missing session, wrong action and dirty generated destinations produce observed JSON-RPC `-32603`; staging survives and a corrected request applies. Stale apply instead returns a successful RPC carrying a domain error, removes staging, and makes the old apply/discard ID invalid; a fresh preview can apply.
- Committed cleanup returns `applied` with writes/audit/cleanup detail and rejects reapply. The failed staging directory survives even after session close; this warning is not an automatic-cleanup guarantee or permission to rerun committed work. The test injects staging-removal failure in an isolated server process, not a real permissions fault or crash.
- Disposition: require both post-promotion cleanup paths, exact injection scope, wire/disk/pending/retry evidence, and explicit residual-cleanup ownership in intake review. Refresh evidence bound to changed implementation files; retain old captures as historical. Failure-catalog correction, remaining adapter faults, durable cleanup/crash policy and broader conformance remain rework.

- Evidence refresh found that `EV-02` rejects passing-to-passing supersession. Preserve the old specification/manifest and passing captures as historical, replace active evidence against the new bindings, and reserve `supersedes` for the failed Windows symlink record. No false failure flag, waiver or validator relaxation is needed.

- Refreshing merged main reapplied Git line-ending conversion to historical capture files. One log used mixed LF/CRLF, so neither uniform conversion reproduced its hash. Original bytes were restored only when content normalized identically to Git and the retained SHA-256 matched; captured metadata now has a scoped `-text` rule, with Git checkout round-trip verification. This preserves evidence bytes without changing prior hashes or test claims.

- Native delivery exposed a shared browser readiness race: the negative `not loading` check returned before `<main>` existed. Trace snapshots still showed `loading` when simulator/performance assertions ran. The helper now waits positively for `ready`; the 100 ms editing budget, other assertions, timeouts and normal parallelism are retained. This distinguishes startup completion from the operation being characterized, without treating an absent element as readiness.

## Failure-catalog correction proposal (2026-11-01)

- Per-operation tracing and twenty-one new as-is tests identify direct/apply ledger and publication contention, staging/audit IO, and direct/preview raw JSON-decoding and SQLite operational errors. Discard wraps cleanup IO in `CompilationError`; preview can stage while the destination ledger lock is held. Unsupported OCI push is wrapped in `CompilationError` rather than exposed as its helper class. An import closure is not an escaping-error set.
- CLI runner characterization confirms raw `json.JSONDecodeError` / `sqlite3.OperationalError` escape its normal Click error presentation. These passing tests record known consumer rework, not desired acceptance. Service programming faults remain unnormalized in the named tests.
- A malformed-ledger preview produces real JSON-RPC `-32603`; repairing the ledger permits preview/apply in the same explicit server session. SQL fault transport and native editor history recovery are not claimed. Source review shows that first-turn failure can precede reply metadata, so a protocol test using a known session ID does not establish the editor's recovery path.
- Modelable proposes corrected descriptions at 2.0 for direct, preview and apply, with discard retained at 1.0. The active 1.0 specification and its passing evidence remain untouched. Proposed metadata validates at architecture, has no passing evidence attached, and preserves three `COMPAT-04` diagnostics; the same-version probe additionally reports three `COMPAT-08` diagnostics.
- Activation remains rework pending CLI/adapter handling decisions, unknown external Python consumers, further catalog/coverage review and explicit evidence/version alignment. The runtime/API implementation is unchanged. Quality, classification, cleanup ownership and formal/crash obligations remain open.
- Disposition: add an explicit correction-proposal/consumer-disposition record, operation-specific error-boundary review and a separation between defect characterization and remediation acceptance to the intake template. No metamodel/schema extension is needed.

## Published-tool onboarding follow-up (2026-11-01)

- Modelable's existing adoption was wired to PyPI `system-spec-ref==0.1.0`. The wheel SHA-256 matched PyPI metadata; all 47 installed package modules/resources and all 44 exported toolkit files matched the wheel. The intact export's 182 local Markdown links resolved.
- Existing project specs remain authoritative under `docs/superpowers/specs/system-spec/`; `tools/system-spec/` holds the exported instructions, schema, verifier and conditional skill guides. Copying an example to a second spec path would duplicate the baseline.
- Installed/offline results match for inventory, compilation verification, proposal architecture, expected compatibility/release rejection, context and impact. The packaged schema matches the historical schema; validation/compatibility/artifact functions are unchanged. Historical evidence/source/verifier pins remain intact. Reference tests: 124 passed, one host-dependent file-symlink skip.
- The installed command reports 0.1.0; the standalone export reports `0.0.0+unknown` when distribution metadata is absent. A separate receipt records distribution/wheel origin, dependency versions and export file hashes. Scoped Git attributes and byte round-trip checks preserve the published snapshot.
- Disposition: clarify existing spec paths, reproducible package/export provenance, standalone version metadata and historical evidence preservation in WIRING. The Modelable export remains the exact published 0.1.0 snapshot; these documentation improvements apply to future toolkit exports. No metamodel/schema change is required.
- Tool wiring does not accept the incompatible contract proposal, advance whole-product adoption, close compilation release or replace native/manual gates. Optional formal-model tools remain separate prerequisites.
