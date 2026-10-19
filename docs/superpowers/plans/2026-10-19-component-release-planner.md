# Component Release Planner Implementation Plan

> **For agentic workers:** Implement in the current session with a fresh review before committing. Steps use checkbox syntax for tracking.

**Goal:** Make the immutable-release policy executable as local, side-effect-free release decisions.

**Architecture:** A stdlib Python module consumes a desired release identity and trusted registry/history observations. It returns publish, reuse, or complete-evidence intent, or rejects unsafe input. A JSON CLI permits local inspection without registry credentials.

**Tech Stack:** Python 3.11+, unittest, JSON.

**Spec:** [Immutable component releases](../../../VERSIONING.md#immutable-component-releases).

## Global Constraints

- Work on a dedicated branch in the main checkout; commit locally, do not push.
- Never replace tags, build images, or mutate registry state from the planner.
- Publishing remains disabled until a real verifier and publication-boundary race protection exist.
- Reuse requires verified build-input identity, not source revision equality or changed-path selection.

## Review Focus

- Missing historical images must fail rather than be rebuilt.
- Partial evidence must preserve the original digest.
- A concurrent tag appearing during recheck must be reused or rejected.
- Registry failures and malformed input must fail closed without echoing untrusted payloads.
- A matching digest string is not cryptographic verification; adapters retain that responsibility.

### Task 1: Executable decisions and local CLI

**Files:** Create `scripts/plan_component_release.py`, `scripts/tests/test_component_releases.py`; update `distribution/README.md`, `docs/component-decomposition.md`, and this plan.

**Interfaces:** `plan_release(request, inspect, history=None)` takes JSON-compatible identity/history records and an observation callback, returning a JSON-compatible intent. `python scripts/plan_component_release.py INPUT.json` plans from supplied observations.

- [x] Write failing policy and CLI tests, including the five review cases above.
- [x] Run `python -m unittest discover -s scripts/tests -p test_component_releases.py`; confirm expected missing-module failure.
- [x] Implement strict identity/observation validation and deterministic decisions, with a second inspection before publish intent.
- [x] Run the focused tests and the existing script regression suite (19 focused, 93 total passed).
- [x] Document the input schema, trust boundary, and remaining publishing gates; independent code review and all four doc-review phases passed.

**Delivery:** Commit the verified local slice; record delivery in Git history. No push in this iteration.
