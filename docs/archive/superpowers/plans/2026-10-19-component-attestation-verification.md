# Component Attestation Verification Implementation Plan

**Goal:** Authenticate both required image attestations against an immutable component digest without claiming complete release-input identity or permitting publication.

**Spec:** [Immutable releases](../../../VERSIONING.md#immutable-component-releases), [deployment](../../../spec/12-deployment.md#1932-independent-component-releases), and [ADR-035](../../../spec/adr/ADR-035-component-independence.md).

**Architecture:** A read-only Python adapter invokes installed GitHub CLI verification twice, for SLSA v1 provenance and SPDX SBOM. Fixed GitHub repository, direct release workflow, OIDC issuer, hosted-runner policy and expected source/signer commit constrain the cryptographic verifier. Strict parsing additionally binds returned statements to the exact component image/digest. Normalized output retains `identity_verified: false` and cannot satisfy the planner's trusted-record interface.

**Constraints:** Local branch and commit only; no push or publishing. No credentials in arguments or diagnostics. No new dependency, workflow mutation, backend change or regression-gate change. Existing `gh` and registry authentication are prerequisites; no login is performed. A trusted local `gh` executable and approved expected source commit are caller responsibilities.

**Review focus:** Wrong workflow/source/host; wrong subject or predicate; duplicate/malformed or absent verification output; upstream errors containing sensitive text; authenticity being confused with complete material identity. Each must have a regression test.

## Tasks

- [ ] Write failing tests in `scripts/tests/test_component_attestations.py`: exact immutable commands/policy, two-predicate success, invalid inputs before execution, malformed/duplicate/empty results, subject/predicate mismatch, missing evidence, timeout/tool failure and diagnostic suppression. Confirm failure before implementation.
- [ ] Implement `verify(component, image_digest, source_revision, *, run=...)` and CLI in `scripts/verify_component_attestations.py`. No raw JSON ingestion CLI, host/repository/workflow override or publication operation. Reject all failures with sanitized diagnostics; retain no raw verification output.
- [ ] Synchronize ADR-035, deployment spec, distribution docs, versioning examples and decomposition status. State limits: no complete input identity, material completeness, SBOM semantic/per-platform validation, authenticated history, main/tag eligibility or race protection.
- [ ] Run focused/full Python script tests, check actual installed CLI flags, exercise live failure on an unpublished component without exposing diagnostics, run all four doc-review phases and obtain independent code review.
- [ ] Commit exact local changes; report actual checks and gaps. Full local CI remains required before any later code push.

**Execution:** Main agent implements, read-only subagent independently reviews. This continues the already authorized local roadmap delivery.
