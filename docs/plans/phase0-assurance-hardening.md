# Phase 0 — Assurance and readiness hardening

**Status:** Specification/process change; no runtime compliance asserted. **Baseline:** Observable `2.0`, now tracking observability-spec v2.6 (`7ce467c`) and system-specification 0.6.0 (`5eb061e`); original Phase 0 review was 2026-11-07. **Scope:** Preserve 293 OBS requirements; do not mark an untested requirement verified.

## 1. Acceptance evidence before implementation

- Each requirement's `Acceptance:` and `Negative:` criteria are independently addressable; no requirement becomes verified because one broad positive test passed.
- `scripts/spec-readiness.py` counts a requirement verified only when all of these hold. `--strict` (run by `scripts/spec-gate.sh`) fails on any violation, and on a `verified` status without valid evidence:
  - the metamodel's own evaluation verifies it for its current `requirement_version`: every criterion covered, no `skipped` record, no unsuperseded failing record;
  - every evidence record names the criteria it covers; a blanket record without `criteria` is a policy violation even though the metamodel accepts it;
  - every record names a `verifier`;
  - the method fits the obligation holder taken from the baseline target: platform → `test`, `analysis`, `model-check`, `proof`; organization or process → `inspection`, `analysis`.
- The readiness rules are regression-tested in `scripts/tests/test_spec_readiness.py`.
- A failed, missing, superseded or stale result cannot be silently replaced by a current passing status. Supersession needs `supersedes` and `supersession_reason`; document any waiver and acceptance decision.
- Acceptance decisions include the business scope, risk, scenario, evidence reference and approving owner; automated checks do not substitute for approval or for an independent verifier.

## 2. Complete experience paths

- `spec/business/experience/ux-workflows.yaml` holds the views, interactions, state transitions and scenarios for UX-01–UX-10.
- Every action a view offers is the trigger of an interaction, including `back`, `refresh` and secondary navigation. Every view-specific state (`pending`, `offline`, `elevated`, `failed`) is reached by a scenario.
- Every failure mode is exercised by a negative scenario of its interaction that ends in the mapped state. Negative scenarios exercise exactly one interaction.
- UX-01 and UX-07 carry the Phase 1/2 authorization negatives (restricted service, restricted evidence, existence-hiding search).
- UX-10 separates the shared-link entry (state `ready` or `unauthorized`), elevation (state `elevated`), elevation expiry (OBS-14-09) and an action under elevation (OBS-25-03).
- `scripts/check-ux-contracts.py` enforces UXC-01..09 on top of the metamodel's EXP rules. It is structural: it cannot prove that a requirement link is semantically right or that a workflow runs in a browser.

## 3. Component isolation — new provider readiness

Apply `system-spec/COMPONENT-ISOLATION.md` to every new internal component, including `observable-catalog`, `observable-health`, and `observable-response`:

1. Declare its owned state, versioned provided/required capabilities, source and test scope before coding.
2. Provide an independent execution harness using injected adapters for each required capability.
3. Publish a programmable mock for each provided capability, with isolated resettable scenarios: success, domain failures, permission denial, missing/stale data, delays and timeouts where applicable.
4. Run the same contract suite against the real provider and mock; mismatches are explicit exceptions, never silently ignored.
5. Demonstrate a consumer can run with only mocks and no real dependent services, then separately verify integration with real providers.

Do not treat mock existence or isolated execution as production correctness. Include the provider and consumer obligations in each phase's detailed handoff plan.

## 4. Authorization before Phase 8

Early phases implement the relevant security semantics before shipping protected functionality. Access control is enforced at the capability/API boundary through `decide-access` (ADR-041), not by hiding UI controls.

| Phase | Minimum authorization gate | Spec traceability |
|---|---|---|
| 1 catalog | Policy decision point; tenant- and role-scoped service discovery; no unauthorized record existence or ownership leakage; fail closed | `req-decide-access-fail-closed`, `req-catalog-access-control`; UX-01 `ux-01-restricted-service`, `ux-01-search-service-restricted` |
| 2 health | Scoping across services/environments; restricted evidence withheld without reporting the service normal; fail closed | `req-health-access-control`; UX-07 `ux-07-restricted-evidence` |
| 3 UX | Shared-link authorization, visible elevation state, direct URL and search denial, keyboard-only critical flows | OBS-25-02, OBS-25-03, OBS-13-05; UX-10 |
| 6 response | Incident state/action authorization, accountable audit trail and safe acknowledgement | OBS-23-05, OBS-24-09; UX-04, UX-06 |
| 7 transaction | Business-purpose-bound lookup, actor/purpose audit and strict customer-data minimization | OBS-31-01..03; UX-03 |

Phase 8 finishes broader privacy/compliance controls but cannot postpone these dependencies.

## 5. Domain model and traceability

Implementation domains are declared in `spec/system.yaml`, orthogonal to layers (ADR-042). Planned components have their domains declared before they exist:

| Planned component | Domain | May use |
|---|---|---|
| `observable-catalog` | `service-model` | identity, service-control, telemetry-storage |
| `observable-health` | `service-assessment` | identity, service-model, telemetry-storage |
| `observable-response` | `incident-response` | identity, service-model, service-assessment, telemetry-storage |

`incident-response` must never use `investigation`, because `investigation` already uses `incident-response` (observable-query requires `evaluate-alerts`). The catalog reads observed service names from `telemetry-storage`, not through `investigation`, for the same reason. Every new cross-domain dependency is added to `uses`, reviewed for cycles (DOM-03) and reflected in `--impact`.

A normal agent handoff records:
`requirement IDs → risks → component/capability → experience interactions → acceptance and negative criteria → tests → evidence → decision`.

## 6. Business intent and versioning

- The upstream specification is vendored unchanged in `spec/business/baseline/v2.5/` (commit `523e682`), with `MANIFEST.sha256` and `SOURCE.md`. A new upstream version is a new sibling directory.
- The fragments in `spec/business/` are derived from the baseline. Requirement `target` and `version`, risk IDs and targets, and visual-reference paths follow declared field policies; every other change or removal is declared in `spec/business/refinements.yaml` with a reason and decision reference. `scripts/check-business-baseline.py` verifies the manifest, rejects undeclared deviations and stale declarations, and allows additions.
- A change to meaning or priority is a formal requirement change needing impact analysis, owner review and re-verification; it is never an agent-only update.
- Retiring the upstream repository (Q9) is blocked until the baseline snapshot is merged on `2.0`. Retiring a repository is not deleting the authoritative rationale.

## 7. Next executable vertical slice

Build and independently test `Catalog → Health → Overview → Investigation` across phases 1–3, with UX-01 and UX-07 as initial end-to-end acceptance. A service remains in the catalog without telemetry, missing evidence renders unknown (not healthy), scope persists during investigation, and unauthorized access fails closed. Test each provider independently through its programmable mock, then verify composed behavior with real providers.

### Pre-implementation checks

```bash
bash scripts/spec-gate.sh
```

No passing runtime UX or business acceptance evidence is asserted by these checks.

## 8. 2.0 re-review (2026-11-07)

A re-review of the hardening commit found gates that were advisory, inverted or missing. Changes:

| Finding | Resolution |
|---|---|
| Readiness accepted skipped and failing evidence; the metamodel accepted blanket evidence | Readiness builds on `verified_requirements()` and adds the blanket, verifier and holder-method rules; `--strict`; regression tests |
| No immutable baseline; derived files labelled "imported" | Vendored baseline with manifest; `refinements.yaml`; `check-business-baseline.py`; Q9 blocked until merged |
| Authorization before Phase 8 was prose only | `decide-access` contract; three P0 refinement requirements gating phases 1 and 2; UX-01/UX-07 negatives |
| UX semantic errors (UX-10 states, unreachable `pending`, swapped UX-06 links, OBS-24-11 on the wrong interaction, unmodeled actions, bundled negatives) | Fixed in `ux-workflows.yaml`; baseline changes declared in `refinements.yaml`; UXC-01..09 |
| Planned components had no domain; latent investigation ↔ incident-response cycle | Domains `service-model` and `service-assessment` declared; rule in section 5 and ADR-042 |
| New checks not wired; spec-only changes ran no gate | `scripts/spec-gate.sh`, mandatory for every change and called by `local-ci.sh` |
| P0 scope 88 vs 99; `--stage release` cannot pass for P0 only | ROADMAP defines 102 P0 and a project exit gate for `2.0.0`; holder taken from the baseline |
| `explore-telemetry-ui` carried 54 requirements while `verified` | Split by area into four web capabilities; statuses lowered to `specified` where unverified requirements now apply |
| ADR status drift, duplicate ADR numbers, health scope creep | ADR-037..041 accepted; ADR-039 narrowed; duplicate 035/036 renumbered to 043/044 |
| Metamodel gaps | [system-spec model feedback](../feedback/system-spec-2.0-model-feedback.md) |


## 9. v2.6 criteria and scoped release adoption

The initial readiness rules now use the metamodel's first-class `Criterion.id`, `Requirement.holder`, `system.evidence_policy.require_criteria`, `system.evidence_methods` and `system.release_scope: [P0]`. New UX-01–UX-10 coverage is imported from upstream with 45 interactions and 91 scenarios; `EXP-09..12` supplements the custom UX completeness checker. The four historical requirements reclassified as specified are listed in [the v2.6 alignment plan](spec-alignment-v2.6.md). Previous Phase 0 evidence is not retroactively upgraded by this document.
