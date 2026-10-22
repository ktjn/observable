# Component Build-Input Identity Implementation Plan

**Goal:** Make the release planner's `input_algorithm`/`input_digest` real: a documented,
versioned fingerprint of a component's declared build inputs computed from committed bytes, with
every external image reference bound to an immutable digest instead of a mutable tag.

**Spec:** [Immutable component releases](../../../VERSIONING.md#immutable-component-releases),
[deployment](../../../spec/12-deployment.md#1932-independent-component-releases), and
[ADR-035](../../../spec/adr/ADR-035-component-independence.md).

**Architecture:** The two OCI recipes reference every base/derived image and the Dockerfile
frontend by `@sha256:` digest (index digests, multi-platform). `scripts/collect_component_inputs.py`
resolves the component's build target from the committed recipe, walks the reachable Dockerfile
stage graph, requires every reachable external image reference to be digest-pinned, and hashes a
canonical object (`component-input-v1`) over the source evidence digest, the recipe digest, the
pinned image set, and the target platforms. Output stays `identity_verified: false`: the declared
inputs are fully enumerated and digest-pinned, but the digests are not authenticated against a
registry and the materials that apt/rustup/crates.io/npm resolve at build time remain mutable.

**Constraints:** Local branch and commit only; no push, build, or publishing. No new dependency
(stdlib only). Recipe edits require updating the reviewed `RECIPES` hashes in
`collect_component_source.py`. No backend change, workflow change, or regression-gate change.

**Review focus:** A mutable tag, an unreachable stage, an unrelated image, a wrong target, a
non-deterministic or platform-independent digest, or a silently missing/extra input must each be
caught by a regression test; the tool must never claim authenticated or complete input identity.

## Tasks

- [x] Pin every `FROM` image and the `# syntax=` frontend in `Dockerfile` and
      `apps/frontend/Dockerfile` by `@sha256:` index digest; update the two reviewed `RECIPES`
      hashes and `RECIPE_TARGETS` in `scripts/collect_component_source.py`.
- [x] Write failing tests in `scripts/tests/test_component_inputs.py`: determinism, source/platform
      sensitivity, reachable-only images, mutable-tag and unknown-layout rejection, missing
      target/`COPY --from` rejection, unresolved-material reporting, and sanitized CLI failure.
      Confirm failure before implementation.
- [x] Implement `scripts/collect_component_inputs.py` (`resolve(...)` + CLI) reusing
      `collect_component_source` and the snapshot builder's version resolution.
- [x] Synchronize `distribution/README.md`, `VERSIONING.md`, `spec/12-deployment.md`, ADR-035, and
      the `docs/component-decomposition.md` status table; state the remaining gaps (registry
      authentication of digests, apt/rustup/registry content, externally supplied build args,
      authenticated history).
- [ ] Run focused and full Python script tests, `git diff --stat`, all four doc-review phases, and
      obtain independent code review.
- [ ] Commit exact local changes; report actual checks and gaps. Full local CI remains required
      before any later code push.

**Execution:** Main agent implements, read-only subagent independently reviews. This continues the
already authorized local roadmap delivery.
