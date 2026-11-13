# Component Snapshot Build Implementation Plan

**Goal:** Build a local component image from exact committed Git bytes and bind its local image ID to source/context evidence.

**Spec:** [Immutable component releases](../../../VERSIONING.md#immutable-component-releases).

**Architecture:** Reuse the Git snapshot reader; create a deterministic Docker context tar with the reviewed recipe inputs, feed it to fixed local-only Buildx routes, then compare image ID/labels/platform with recorded metadata. Source inventory and broader Docker context are recorded separately.

**Constraints:** Commit locally, no push. Existing developer builds stay unchanged. No arbitrary Docker flags, registry destination or CI binary route. Context comes from Git objects, not checkout or Git archive filters. Complete external-material identity and authenticated release evidence remain open; no ADR change is required for this implementation of accepted policy.

### Committed local builds

- [x] Inspect Docker availability and COPY/cargo-chef constraints; independently review context selection.
- [x] Write failing context, local command, output and failure-boundary tests.
- [x] Extract reusable Git reader without changing source digest semantics; implement deterministic context and fixed build/inspect routes. Review identified mutable-request and stale-metadata gaps; failing regressions preceded both fixes.
- [x] Run the script suite (144 tests passed) and an actual local web snapshot build; verify image labels and metadata. Rust/arm64 build limits are stated explicitly.
- [x] Independently review code/docs and reconcile validation before committing. All four doc-review phases, local links/anchors, and whitespace checks passed; no remaining review findings.

Actual web verification: source `1e29b311`, `linux/amd64`, Buildx/Docker Engine 29 containerd store. Build succeeded; initial classic-store verification failed honestly. Added regression-backed descriptor/root-ID binding while retaining classic config-ID equality. Corrected build passed, then standalone index and referenced JS asset returned HTTP 200; ephemeral container stopped. API host mappings were fixture-only, with no backend acceptance claim. Rust/arm64 builds were not run.

Files: `scripts/collect_component_source.py`, `scripts/build_component_snapshot.py`, `scripts/tests/test_component_snapshot_build.py`, `distribution/README.md`, `docs/component-decomposition.md`.

Review focus: dirty/untracked data; export attributes; unrelated Cargo source needed by cargo-chef; unsafe tar names/modes; stale or changed context/metadata; build failure cannot produce success; local labels are not cryptographic release trust.

**Delivery:** Git history records the local commit. No publishing.
