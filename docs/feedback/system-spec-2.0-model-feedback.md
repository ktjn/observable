# system-spec model feedback from the Observable 2.0 adoption

**Source:** Observable `2.0` re-review, 2026-11-07. **Toolkit:** `system-spec-ref 0.5.0`. **Target:** `ktjn/system-specification` (CONSTITUTION.md, Self-Improvement and Model Feedback).

Each item names the gap, the local workaround in this repository and a minimal metamodel change.

## MF-1: No obligation holder on requirements

- **Gap:** `Requirement` has no field for who carries the obligation (platform, organization, process). Evidence method rules depend on it: an organizational obligation cannot be verified by an automated test.
- **Workaround:** holder is derived from the baseline target (`<area>.<holder>`) and duplicated in `docs/plans/business-spec-gap.yaml`; `scripts/spec-readiness.py` enforces the method per holder.
- **Proposal:** add `Requirement.holder` (free reference or `platform | organization | process`) and an invariant mapping holder to allowed `Evidence.method`, configurable in `system`.

## MF-2: Release stage cannot be scoped by priority

- **Gap:** STAT-04, REQ-05 and RISK-04 apply to every active requirement. A release that delivers only P0 cannot pass `--stage release` while P1/P2 stay active.
- **Workaround:** the `2.0.0` exit is a project gate in ROADMAP.md (`spec-readiness.py --priority P0 --strict --fail-below 102`); release-stage findings for P1/P2 are listed in the gate decision record.
- **Proposal:** `--release-scope priority=P0` (or `system.release_scope`) that limits STAT-04/REQ-05/RISK-04/EXP-07 to in-scope requirements and reports out-of-scope ones as deferred, not passed.

## MF-3: Blanket evidence verifies a multi-criterion requirement

- **Gap:** an evidence record with empty `criteria` covers the whole requirement. One broad test can verify every acceptance and negative criterion.
- **Workaround:** `spec-readiness.py --strict` rejects blanket records for `OBS-*` and refinement requirements.
- **Proposal:** `system.evidence_policy.require_criteria: true` (or per-requirement), making empty `criteria` an EV error at verification stage.

## MF-4: Criteria are bound by exact text

- **Gap:** `Evidence.criteria` must repeat the criterion string. A wording fix orphans evidence (EV-06 catches it, but the binding is fragile and verbose).
- **Proposal:** optional criterion IDs (`criteria: [{id: neg-1, text: ...}]`) with evidence referencing IDs; text changes then become an explicit version bump rather than an accidental orphan.

## MF-5: Experience rules miss reachability and isolation

- **Gap:** EXP-03..05 and EXP-08 do not detect view actions that no interaction uses, view-specific states no scenario reaches, failure modes whose mapped state no negative scenario reaches, or negative scenarios that bundle several interactions so a failure state has no single cause.
- **Workaround:** `scripts/check-ux-contracts.py` rules UXC-02..08.
- **Proposal:** add these as EXP-09..EXP-12 at verification stage for `critical` + `exhaustive` experiences.

## MF-6: No baseline import with declared deviations

- **Gap:** `SpecImport` references another spec's capabilities but cannot import its requirements, risks and experiences as a pinned, digest-checked baseline with declared local refinements.
- **Workaround:** vendored baseline with `MANIFEST.sha256`, `spec/business/refinements.yaml` and `scripts/check-business-baseline.py`.
- **Proposal:** `SpecImport.mode: baseline` with `artifacts` digests and a `refinements` list (path, reason, decision); validator reports undeclared changes and removals.

## MF-7: `state_sets` in include fragments (open upstream as OBSSPEC-EXP-09)

- **Gap:** include fragments cannot declare `state_sets`, so the root file must.
- **Proposal:** allow `state_sets` in fragments with duplicate-name detection.
