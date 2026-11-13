# Observable 2.0 Adoption Feedback

Date: 2026-11-07.
Target system: [`ktjn/observable`](https://github.com/ktjn/observable) branch `2.0` ([#841](https://github.com/ktjn/observable/pull/841), [#842](https://github.com/ktjn/observable/pull/842)).
Baseline: [`ktjn/observability-spec`](https://github.com/ktjn/observability-spec) v2.5 (`523e682`), adopted as the 2.0 requirement baseline.
Toolkit: `system-spec-ref` 0.5.0.

## Context

Observable merged a 293-requirement business specification (37 areas, UX-01..UX-10) into its implementation specification and switched to a verification-stage gate. A re-review of the Phase 0 hardening found gates that were advisory, inverted or missing. Several causes were metamodel or tooling gaps; this record lists those. Project-specific policy stays in Observable (`docs/plans/phase0-assurance-hardening.md` § 8).

## Findings

| ID | Observed fact | Gap | Resolution |
|---|---|---|---|
| OBS2-FB-01 | `--stage release --check-drift` printed `VALID` for a spec that fails release; Observable's AGENTS.md documented running the stage separately as a workaround | Check flags selected a separate diagnostic path and dropped explicit `--stage`/`--compat` | Fixed here: explicit `--stage` or `--compat` now runs with any `--check-*` flag and errors are reported together |
| OBS2-FB-02 | 14 view actions (`back`, `refresh`, secondary navigation) had no interaction while experiences claimed `exhaustive` | No rule ties view actions to interactions | `EXP-09` |
| OBS2-FB-03 | `pending` states existed for acknowledgement but no scenario reached them, so "pending vs completed" (a P0 requirement) was unexercised | No reachability rule for view-specific states | `EXP-10` |
| OBS2-FB-04 | A negative scenario existed for an interaction, but not one ending in each failure mode's mapped state | `EXP-05` only requires some negative scenario | `EXP-11` |
| OBS2-FB-05 | Negative scenarios bundled two interactions and two failure states, so no failure state had a single cause; a positive scenario ended in a state only another interaction produces | No attribution rule | `EXP-12`; guidance in EXPERIENCE.md |
| OBS2-FB-06 | Wrong requirement links (swapped UX-06 requirements, OBS-24-11 on a non-refresh interaction) were found only by reading | Structural rules cannot judge semantics | Documented limit in EXPERIENCE.md; review obligation |
| OBS2-FB-07 | A project readiness script reimplemented evidence evaluation and diverged in both directions from the validator | No public evidence report; blanket evidence verifies all criteria | Guidance in FEATURE.md; `--evidence-report` and `EV-08` ([#113](https://github.com/ktjn/system-specification/issues/113)) |
| OBS2-FB-08 | `--stage release` cannot pass for a P0-only release while P1/P2 stay active | `priority` has no effect on invariants | `release_scope` / `--release-scope` ([#114](https://github.com/ktjn/system-specification/issues/114)) |
| OBS2-FB-09 | Organizational and process obligations need inspection evidence; the holder was only recoverable from the upstream `target` | No holder concept | `Requirement.holder`, `evidence_methods`, `EV-07` ([#115](https://github.com/ktjn/system-specification/issues/115)) |
| OBS2-FB-10 | Evidence binds to exact criterion text | Fragile binding | `Criterion` IDs, `REQ-10` ([#116](https://github.com/ktjn/system-specification/issues/116)) |
| OBS2-FB-11 | The upstream spec was merged and edited in place; derived files were labelled "imported"; the upstream repo was scheduled for deletion | No baseline-with-deviations import | GREENFIELD.md § Adopting an upstream specification; `SpecImport.mode: baseline` ([#117](https://github.com/ktjn/system-specification/issues/117)) |
| OBS2-FB-12 | Planned components could not be placed in domains before they exist; a latent `investigation` ↔ `incident-response` cycle was invisible | DOM rules see only declared components | GREENFIELD.md § Planned components |
| OBS2-FB-13 | The project exempted docs/spec-only changes from its CI script, which was the only place the spec gate ran | Process gap | FEATURE.md rule |
| OBS2-FB-14 | Include fragments cannot declare `state_sets` | Open upstream as OBSSPEC-EXP-09 | Unchanged |

## Not folded back

- Holder-to-method values, requiring a `verifier`, the P0 count and Observable's exit gate.
- Observable's trigger floor per workflow and `preserves` vocabulary (project vocabulary).
- Domain names, the authorization contract and ADR housekeeping.

## Compatibility of this change

`EXP-09`..`EXP-12` are new verification-stage invariants for experiences marked `critical` (`EXP-09`/`EXP-10` only with `exhaustive`). Specs that use experiences can need new scenarios or interactions. The observability-spec v2.5 baseline fails them; Observable's derived workflows pass. The CLI change affects only commands that combine `--check-*` with an explicit `--stage` or `--compat`.
