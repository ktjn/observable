# Component Registry Inspection Implementation Plan

**Goal:** Obtain live, read-only manifest presence and integrity evidence without treating it as verified release identity.

**Spec:** [Immutable releases](../../../VERSIONING.md#immutable-component-releases) and [OCI manifest pulls](https://github.com/opencontainers/distribution-spec/blob/main/spec.md#pulling-manifests).

**Architecture:** Fixed GHCR component/version requests through a bounded HTTPS transport, strict response classification, raw-byte digest verification, and separately typed found evidence. Scoped bearer tokens may be supplied in memory/stdin; no credential retrieval or publishing.

**Constraints:** Local branch and commit only. No redirects or registry writes. No new dependencies or runtime service changes. No ADR change: implements accepted policy; source/build identity, attestation verification, history and race protection remain separate gates.

### Inspector

Files: `scripts/inspect_component_registry.py`, `scripts/tests/test_component_registry_inspection.py`, `distribution/README.md`, `docs/component-decomposition.md`.

- [x] Inspect policy, existing planner and primary OCI protocol reference; independently scope trust and regression boundaries.
- [x] Write failing local HTTP-fixture tests and confirm missing-module failure.
- [x] Implement bounded GET-only transport, strict manifest/error validation and sanitized CLI. Add failing slow-header/trailer and stdin-limit regressions before fixing the review findings; 18 focused tests passed.
- [x] Run focused and full script tests (18 inspector tests; 130 total passed). Independent code review and all four doc-review phases passed with no remaining findings; local links, anchors and whitespace checks passed.

Review focus: ambiguous missing responses; raw versus reserialized digest; mixed authorization errors; oversized/truncated responses; duplicate headers/JSON; credential redirection or diagnostic leakage; planner cannot promote found integrity evidence into trusted identity.

**Delivery:** Commit after verification. No push or live GHCR requests in this iteration.
