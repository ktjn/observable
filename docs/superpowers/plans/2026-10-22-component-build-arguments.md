# Component Build-Argument Binding Implementation Plan

**Goal:** Bind effective Docker build arguments to declared component identity and the local snapshot image actually built.

**Spec:** [Immutable releases](../../../VERSIONING.md#immutable-component-releases), [deployment](../../../spec/12-deployment.md#1932-independent-component-releases), [ADR-035](../../../spec/adr/ADR-035-component-independence.md).

**Architecture:** Advance the incomplete declared-input algorithm to `component-input-v2` by adding an effective `build_args` map. Resolve only the reviewed reachable ARG declarations: fixed Rust SERVICE mapping, or the web mode/OTLP-endpoint defaults plus supported explicit overrides. Move shared version lookup into a neutral metadata module to remove the collector-to-builder import cycle. Snapshot preparation saves input evidence; execution recomputes that evidence from the same in-memory tar recipe and recorded source assertions before emitting explicit `NAME=VALUE` Docker arguments, labels and a local tag bound to source/platform/input identity.

**Constraints:** Local branch/commit only, no push. No dependency, Dockerfile, Rust, UI or protected regression-gate changes. Values are public configuration, never credentials. All evidence stays `identity_verified: false`; registry authentication, complete mutable-material resolution, history and publication races remain open. Old v1 fingerprints and old prepared attempts are not silently promoted.

**Review focus:** Explicit/implicit defaults must match; different effective values must change identity; unknown/duplicate arguments, URL credentials and control characters fail before persistence or Docker; neither bare host-inheriting arguments nor a different SERVICE may execute; inconsistent request/evidence/labels must fail closed. Preserve all existing context, image and linkage checks.

## Tasks

- [x] Add failing input/snapshot tests for defaults, overrides, order/equivalence, argument rejection, Docker invocation, evidence recomputation, labels and local-tag separation. Review regressions for tab-separated ARG declarations and caller mutation also failed before correction and then passed.
- [x] Implement shared version metadata, v2 argument resolution/CLI and canonical identity; integrate preparation and execution with consistent `inputs.json`, request and labels.
- [x] Run focused regressions and actual committed-recipe collection/preparation. Default and overridden web inputs at `b5d63925` produced matching collector/request evidence, unchanged source/context and distinct input digests/local tags. Fresh-process imports passed in both module orders.
- [x] Run full script suite and final independent code review. All 182 script tests passed; the fresh reviewer confirmed both code fixes and no remaining code findings.
- [ ] Run a real local web build with supported overrides and inspect the argument-bearing bundle and image labels. Blocked on this host: Docker's configured engine pipe is absent; Docker Desktop is not running/available. Earlier snapshot execution evidence predates this v2 path and cannot close this check.
- [x] Synchronize ADR/spec, versioning, distribution docs, roadmap and agent orientation; pass four-phase doc review and whitespace/link checks. A stale v1 reference was corrected before final independent doc-review PASS.

**Delivery evidence:** The scoped local commit is recorded by Git history. No push is authorized. Full local CI was not run and is required before any later code push. Testcontainers is not applicable: this slice changes local build tooling, not backend dependency behavior; command/evidence fixtures and committed-source preparation are the replacement signals, with actual image execution still open above.
