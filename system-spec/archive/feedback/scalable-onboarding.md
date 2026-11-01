# Scalable Brownfield Re-onboarding Feedback

Date: 2026-11-01.
Target: [`ktjn/scalable`](https://github.com/ktjn/scalable) (`6f1eb2cb`), `spec-model/` (PRs ktjn/scalable#994, ktjn/scalable#995).
Metamodel: `0c562dd`.

Outcome: Levels 0-2 pass for framework and marketplace. Framework Level 3 reopened: one live-cluster test fails locally, so `--stage release` fails `REQ-04`.

## Findings

| ID | Observed fact | Gap | Proposed change |
|---|---|---|---|
| SCL-FB-01 | `scalable.yaml`: 12 components in 11 layers, nearly 1:1; marketplace splits `domain`/`types` into two layers only to express a dependency direction. | `Component.layer` is one flat, ordered axis carrying both dependency direction (`COMP-05`) and cohesion grouping. No domain/context concept. | Add an unordered `Component.domain` (bounded context); keep `layer` for direction only. `--diagram` groups by domain; optional per-domain `context_budget`. |
| SCL-FB-02 | Scalable's layer map forbids dependencies between sibling layers of equal rank (`transactions`/`hosting` both rank 6), and the model collapses ranks. `COMP-05` allows same-layer dependencies. | The metamodel cannot express "siblings may not depend on each other", so the stricter product rule is unchecked by the validator. | Optional `System.layer_ranks` (equal rank = siblings) and a check that same-rank layers do not depend on each other. |
| SCL-FB-03 | Evidence for Rust crates is `nextest::-p <crate>` selectors. `test_paths`/`COMP-08` require provenance files under component paths; `fakes`/`COMP-09` assume per-capability fakes. | `test_paths`/`fakes` fit Python file-per-test layouts, not package selectors. Not adopted. | Allow `test_paths` to match selector provenance (package/target) or document the Rust mapping. |
| SCL-FB-04 | `--check-drift` scans Python imports and JSON Schema fields only; Rust Cargo wiring drift was checked by hand (manifest diff + `check-layering.py`). | Cargo dependencies vs `requires`/`bindings` unchecked. | Optional Cargo-metadata drift adapter, or document the product-checker pattern. |
| SCL-FB-05 | One failing test (`the_fm_moves_a_replica_of_a_loaded_module_onto_an_idle_node`) in a 2255-test crate-level selector. Evidence is per crate, so the whole requirement fails; excluding the test would be a skip, which cannot verify the requirement. | No per-test criterion mapping; a single flaky/environmental failure cannot be scoped or waived below the release stage. | Allow `Evidence.criteria` mapping or a scoped, expiring evidence waiver recorded as risk. |
| SCL-FB-06 | Upstream CI only builds; tests run via `scripts/local-ci.sh`. A green remote run is not test evidence. | `verifier` is free text, so local vs CI provenance is not distinguished. | Record `environment` (host/OS) on `Evidence`; reviewers can then compare failing local and passing remote runs. |

## Dispositions

- Implemented: SCL-FB-03 (selector entries in `test_paths`/`fakes`, `COMP-08`/`COMP-09`) and SCL-FB-04 (Cargo path/workspace dependency drift in `--check-drift`).
- Implemented: SCL-FB-01 as n:m domains: `system.domains` (`uses`), `Component.domain`, `DOM-01`–`DOM-03`, nested in `--diagram`. SCL-FB-02 is covered by domains (equal-rank siblings = same-layer domains without `uses`) instead of `layer_ranks`.
- Partly implemented: SCL-FB-06 (optional `Evidence.environment`; no invariant yet).
- Implemented: SCL-FB-05 as `Evidence.criteria` + `EV-06`: per-criterion evidence makes a single failing test attributable to its criterion. A failing criterion still blocks release (no waiver below it); excluding the test would remain a skip.
- Product-side: the failing test is tracked in `spec-model/INTAKE.md`; needs a fix or confirmation that it is environmental.
