# Rust Snapshot Runtime Verification Implementation Plan

**Goal:** Require Rust snapshot images to pass isolated dynamic-library verification before recording local build success, and close the previously unexecuted amd64 Rust image build check.

**Spec:** [Committed snapshot builds](../../../distribution/README.md#committed-snapshot-builds) and [immutable release policy](../../../VERSIONING.md#immutable-component-releases).

**Architecture:** Keep source/context, metadata, label and platform checks. Run the mapped Rust binary through the loaded image's `ldd`, using its verified image ID in an ephemeral container with networking disabled. Only successful linkage adds a runtime check to the result. No service dependency or published-artifact acceptance is implied.

**Constraints:** Local commit only, no push. No Rust/UI/dependency changes. No ADR change: verification of the already accepted image boundary. Existing success assertions stay intact; linkage adds a signal.

### Runtime verification

- [x] Build all seven Rust amd64 snapshots from commit `89479d10`; image metadata/labels/platform checks passed.
- [x] Independently run linkage checks for all seven images; each `ldd` exited 0 with no missing libraries. All ephemeral containers auto-removed.
- [x] Add failing regressions for linkage failure and image-ID execution before implementation.
- [x] Require linkage verification in the builder and run focused/full script tests. Final script suite: 146 tests passed; focused regression tests confirmed rejection before implementation and success after the final pull-prohibition change.
- [x] Recheck the automated path against actual images; independently review code/docs. All four doc-review phases and final pull-prohibition review passed, with no remaining findings. Local Markdown links/anchors and whitespace checks passed.

Actual follow-up: automated build plus linkage passed for all seven Rust images from `89479d10`. The final `--pull=never` path was also checked against all seven verified local image IDs, with no missing libraries. This does not establish service startup, arm64 or cluster acceptance.

Files: `scripts/build_component_snapshot.py`, `scripts/tests/test_component_snapshot_build.py`, `distribution/README.md`, `docs/component-decomposition.md`.

Review focus: `ldd` may exit 0 with missing libraries; never launch by a mutable tag after checking an image ID; keep network disabled; failure cannot leave a success record; web checks remain distinct; arm64 and service/cluster readiness remain unverified.

**Delivery:** Commit locally after final script-suite completion; Git history records delivery. No push.
