# Phase 0 — Assurance and readiness hardening

**Status:** Specification/process change; no runtime compliance asserted. **Baseline:** Observable `2.0` at 2026-11-07; imported observability-spec v2.5 (`523e682`). **Scope:** Preserve 293 OBS requirements; do not mark an untested requirement verified.

## 1. Acceptance evidence before implementation

- Each P0 requirement's `Acceptance:` and `Negative:` criteria remain independently addressable; no requirement becomes verified because one broad positive test passed.
- For P0, every criterion needs an exact `Evidence.criteria` match, current `requirement_version`, passing result and reviewable `provenance`. Evidence can be `test`, `analysis`, or accountable `inspection` according to the obligation holder and risk. Independent verifier/owner approval is project policy, not inferred from metamodel validity.
- A failed, missing, superseded or stale result cannot be silently replaced by a current passing status. Document any waiver and explicit acceptance decision.
- `scripts/spec-readiness.py` is an informational progress report, now requiring full criterion coverage. Formal stage and version checks remain the authority in `system-spec-ref`.
- Acceptance decision must include the business scope, risk, scenario, evidence reference and approving owner; automated checks do not substitute for approval.

## 2. Complete experience paths

- `spec/business/experience/ux-workflows.yaml` contains authoritative views, interactions, state transitions and scenarios for UX-01–UX-10.
- UX-02 covers selecting an interval, refining filters, comparing baseline and drilling into evidence.
- UX-03 covers purpose-bound search, opening an authorized transaction, and inspecting steps and unknown outcomes.
- UX-04 covers notification/list entry, acknowledgement, inspecting evidence, recording a hypothesis and retry/recovery.
- UX-06 covers opening a mobile notification, acknowledgement with disconnection and reconciliation/retry.
- UX-10 covers shared-link denial, restricted search, time-bound elevation and evidence inspection.
- `scripts/check-ux-contracts.py` enforces these minimum interaction triggers and positive/negative scenario coverage without assuming a UI framework. It cannot prove workflows execute in a browser.

## 3. Component isolation — new provider readiness

Apply `system-spec/COMPONENT-ISOLATION.md` to every new internal component, including `observable-catalog`, `observable-health`, and `observable-response`:

1. Declare its owned state, versioned provided/required capabilities, source and test scope before coding.
2. Provide an independent execution harness using injected adapters for each required capability.
3. Publish a programmable mock for each provided capability, with isolated resettable scenarios: success, domain failures, permission denial, missing/stale data, delays and timeouts where applicable.
4. Run the same contract suite against the real provider and mock; mismatches are explicit exceptions, never silently ignored.
5. Demonstrate a consumer can run with only mocks and no real dependent services, then separately verify integration with real providers.

Do not treat mock existence or isolated execution as production correctness. Include the provider and consumer obligations in each phase's detailed handoff plan.

## 4. Authorization before Phase 8

Early phases must implement the relevant security semantics before shipping protected functionality:

| Phase | Minimum authorization gate |
|---|---|
| 1 catalog | Tenant- and role-scoped service discovery; no unauthorized record existence or ownership leakage |
| 2 health | Scoping across services/environments; uncertainty about missing evidence visible without leaking restricted observations |
| 3 UX | Shared-link authorization, visible elevation state, direct URL and search denial, keyboard-only critical flows |
| 6 response | Incident state/action authorization, accountable audit trail and safe acknowledgement |
| 7 transaction | Business-purpose-bound lookup, actor/purpose audit and strict customer-data minimization |

Phase 8 finishes broader privacy/compliance controls, but cannot postpone these dependencies. Access controls must be enforced at the capability/API boundary, not by hiding UI controls.

## 5. Domain model and traceability

Implementation domains are declared in `spec/system.yaml`, orthogonal to layers. Every cross-domain dependency must be represented in `uses`, reviewed for cycles and reflected in `--impact` during feature changes. Domain assignments initially cover existing eight runtime components only; planned components receive domain assignments when introduced. See ADR-042.

A normal agent handoff records:
`requirement IDs → risks → component/capability → experience interactions → acceptance and negative criteria → tests → evidence → decision`.

## 6. Business intent and versioning

- Preserve source identity, requirement IDs and the imported v2.5 baseline commit `523e682`.
- Keep the upstream snapshot immutable as a historical reference; refinements that connect requirements to runtime capabilities live in the implementation spec.
- A change to meaning or priority is a formal requirement change needing impact analysis, owner review and re-verification; it must not be an agent-only update.
- When the upstream repo is retired, keep Git history and source provenance accessible. Retiring a repository is not deleting the authoritative rationale.

## 7. Next executable vertical slice

Build and independently test `Catalog → Health → Overview → Investigation` across phases 1–3, with UX-01 and UX-07 as initial end-to-end acceptance. A service remains in the catalog without telemetry, missing evidence renders unknown (not healthy), scope persists during investigation, and unauthorized access fails closed. Test each provider independently through its programmable mock, then verify composed behavior with real providers.

### Pre-implementation checks

```bash
uv run --project models system-spec spec/system.yaml --stage verification
python scripts/check-ux-contracts.py
uv run --project models python scripts/spec-readiness.py --priority P0
```

No passing runtime UX or business acceptance evidence is asserted by these checks.
