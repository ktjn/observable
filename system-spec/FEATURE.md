# Feature: Changing an Already-Specified System

Contract-first sequencing for adding or changing behavior in a system that already has a validated spec (built via [greenfield](GREENFIELD.md) or onboarded via [brownfield](BROWNFIELD.md) at Level 1+). Artifact rules: [constitution](CONSTITUTION.md). Load this document together with the constitution, the applicable skill and one component slice only.

## Steps

| # | Step | Output | Check |
|---|---|---|---|
| 1 | Baseline | Reviewed spec revision pinned as the old spec (e.g. a Git revision saved as `old.yaml`) | `<spec> --stage <current>` is `VALID` before any change |
| 2 | Scope | Capability delta, affected IDs, target components in dependency order (providers first) | `--impact <id>`; `--plan <id>` for provider-first waves (components in a wave can run as parallel agents) |
| 3 | Contract | New or changed capabilities, interfaces and data models; versions bumped | `--compat old.yaml`; a break needs consumer migration, a major bump and prior `deprecated` status for removals |
| 4 | Boundary | Owning component per change; `provides`, `requires`, `fakes`, `test_paths`; split if over budget | `--stage architecture` |
| 5 | Verification spec | New/changed requirements in every affected category; evidence obligations | `--stage verification` |
| 6 | Invalidate | Evidence for changed requirements, subjects or dependencies marked historical until rerun | `--impact` lists `invalidated_evidence` |
| 7 | Handoff | One bounded `--context <component-id>` slice per component | Slice contains the contracts, fakes and `source_paths`/`test_paths` |
| 8 | Implement | Code under `source_paths`; tests under `test_paths` against the fakes | Tests fail first where behavior is missing; `--check-scope <id> --changed -` (fed by `git diff --name-only <base>`) reports nothing outside the component |
| 9 | Verify | Fresh results per requirement; provider contract tests for each fake | `--check-artifacts --root`, `--check-drift --root`, `--stage release` |
| 10 | Accept | Gate decision; new evidence bound to subject versions/revisions | Failures kept (`supersedes` with reason), no waivers at release |

Rules:

- Do not change implementation first. If code already exists, bring the spec to the new behavior and re-derive tests before merging.
- One component per handoff, enforced with `--check-scope <component> --changed <file|->` (path list such as `git diff --name-only <base>`; `--allow <path>` for lockfiles or docs). Spec edits belong to the contract step; in a split spec a component's requirements and evidence live in its own fragment (pass it with `--allow spec/<component>.yaml`). A feature spanning components is delivered provider-first, each with its own slice, fakes and tests; a consumer never imports the provider in tests (`--check-drift`).
- Reopen the earliest affected gate and record it in the [gate decision record](CONSTITUTION.md#gate-decision-record); do not relabel an incomplete artifact to advance.
- A fake is only as good as its provider's contract tests; the change is not accepted until the provider verifies the behavior the fake models.
- The implementer returns a handoff return (`reference-implementation/handoff-return.schema.json`): `--check-return <file>` verifies every requirement of the component is a result or unmet, versions are current, failures are unmet, and changes stay in scope; its `results` map 1:1 to `Evidence` records for the component's fragment.
- Hidden coupling, undocumented failure modes or metamodel gaps found along the way go to [feedback](CONSTITUTION.md#self-improvement-and-model-feedback), not into a workaround.

## Gate Review

| Gate | Bring to review | Decision required before proceeding |
|---|---|---|
| Scope | Requested behavior, `--impact` result, target components and order | The change is bounded; affected consumers and evidence are known |
| Contract | Spec diff, `--compat` result, migration/deprecation plan | Compatible, or every consumer has a migration with recovery; versions follow the compatibility result |
| Verification | Requirement diff, evidence to rerun, new fakes and contract tests | Each changed obligation has a way to establish pass/fail |
| Acceptance | Requirement-linked results for current revisions, drift/artifact checks, risk dispositions | All affected obligations pass, no stale evidence, no waiver at release |

## Prompts

- **Scope**: "Given this request and the current spec, name the capability delta and run `--impact` on each affected ID; list components, consumers and evidence invalidated. No design yet."
- **Contract**: "Propose the spec change (capabilities, interface, data models, versions) for `<component>`; show the `--compat` result against the old spec and any migration needed. Do not invent fields."
- **Verification**: "Write requirements and acceptance criteria for the changed obligations, the fake for each newly required capability, and the provider contract tests that keep the fake honest."
- **Implement**: "Implement this `--context <component-id>` slice within `source_paths`; write tests under `test_paths` that depend only on the fakes. Return requirement-linked results and any new facts or model gaps."

Example of the component layout (source, tests, fakes, schema, checks): [`examples/components/`](examples/components/README.md).
