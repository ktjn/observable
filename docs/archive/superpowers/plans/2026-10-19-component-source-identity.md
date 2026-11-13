# Component Source Identity Implementation Plan

**Goal:** Collect repeatable source evidence for immutable component releases from committed Git objects.

**Architecture:** A source-only collector reads historical manifests through the existing dependency graph loader, inventories selected blobs with modes and SHA256 content hashes, and hashes canonical JSON. Complete release identity still requires external build materials and registry verification.

**Spec:** [Immutable releases](../../../VERSIONING.md#immutable-component-releases).

**Constraints:** Local branch and commit only; no push, registry access, image build, or generated-file edits. No ADR update is required because this implements the accepted release policy without changing ownership or deployment.

### Source collector

Files: `scripts/collect_component_source.py`, `scripts/ci_changed_components.py`, `scripts/tests/test_component_source.py`, `distribution/README.md`, `docs/component-decomposition.md`.

- [x] Scope Docker COPY inputs, workspace metadata, transitive source dependencies, web context, and external-material gaps with an independent reviewer.
- [x] Write policy tests and confirm missing-module failure.
- [x] Implement historical tree reads, strict graph loading, canonical source digest, and recipe-review guard.
- [x] Run focused tests and the full script suite (112 tests passed); collect all eight real component snapshots (54–358 files).
- [x] Independently review code and docs; fix the two review findings with failing regressions first. All four doc-review phases passed.

**Delivery:** Commit locally after validation; Git history records the resulting commit. No push.

Review focus: historical source despite dirty checkout; docs-only commits; unrelated service source versus all workspace metadata; missing/unsupported files, symlinks and submodules; recipe/layout changes; no claim of verified external build identity.
