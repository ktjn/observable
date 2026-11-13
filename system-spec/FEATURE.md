# Feature: Changing an Already-Specified System

Contract-first sequencing for adding or changing behavior in a system that already has a validated spec (built via [greenfield](GREENFIELD.md) or onboarded via [brownfield](BROWNFIELD.md) at Level 1+). Artifact rules: [constitution](CONSTITUTION.md). Load this document together with the constitution, the applicable skill and one component slice only.

## Specification authority

For a previously accepted scope, define the intended behavioral delta against the reviewed specification before implementation. Existing code is not authority for silently extending contracts. A change that reveals undocumented behavior requires an explicit decision to preserve, reject or leave it unconstrained. If the aim is to replace code without changing expected behavior, use [REPLACEMENT.md](REPLACEMENT.md).

## Isolation-aware changes

When [component isolation](COMPONENT-ISOLATION.md) is adopted, update the provider's published programmable mock and its contract tests as part of any changed capability. Verify consumers against the mock and the provider independently before the integration join. Changing a mock to match a defective implementation does not amend the accepted contract.

## Steps

| # | Step | Output | Check |
|---|---|---|---|
| 1 | Baseline | Reviewed spec revision pinned as the old spec (e.g. a Git revision saved as `old.yaml`) | `<spec> --stage <current>` is `VALID` before any change |
| 2 | Scope | Capability delta, affected IDs, target components in dependency order (providers first) | `--impact <id>`; `--plan <id>` for provider-first waves (components in a wave can run as parallel agents) |
| 3 | Contract | New or changed capabilities, interfaces and data models; versions bumped | `--compat old.yaml`; a break needs consumer migration, a major bump and prior `deprecated` status for removals |
| 4 | Boundary | Owning component per change; `provides`, `requires`, `fakes`, `test_paths`; split if over budget | `--stage architecture` |
| 5 | Verification spec | Feature requirement decomposed into one sub-requirement (`refines`) per component ([below](#decompose-and-hand-off)); new/changed requirements in every affected category; evidence obligations | `--stage verification` |
| 6 | Invalidate | Evidence for changed requirements, subjects or dependencies marked historical until rerun | `--impact` lists `invalidated_evidence` |
| 7 | Handoff | One bounded `--context <component-id>` slice per component, given to one implementer (person, agent or session) | Slice contains the contracts, fakes and `source_paths`/`test_paths` |
| 8 | Implement | Code under `source_paths`; tests under `test_paths` against the fakes | Tests fail first where behavior is missing; `--check-scope <id> --changed -` (fed by `git diff --name-only <base>`) reports nothing outside the component |
| 9 | Verify | Fresh results per requirement; provider contract tests for each fake | `--check-artifacts --root`, `--check-drift --root`, `--stage release` |
| 10 | Accept | Gate decision; new evidence bound to subject versions/revisions | Failures kept (`supersedes` with reason), no waivers at release |

Rules:

- Decompose the feature's requirement into sub-requirements (`refines`) so each targets one component; that is the unit of handoff. Components are developed and verified separately against fakes, then joined: the parent requirement needs its own integration evidence (a `test_scope: integration` component) and every sub-requirement verified (`REQ-08`); iterate by superseding failed evidence until the parent holds.
- Test-first (TDD) unit tests bind to the component sub-requirement whose criterion they elaborate, not to the feature requirement; add a criterion rather than leave a test unbound.
- Tests trace to exactly one requirement: bind them with a `verifies` marker/annotation and check run reports with `--check-report`; derive `test` evidence with `--emit-evidence` instead of writing `passed` by hand.
- Do not change implementation first. If code already exists, bring the spec to the new behavior and re-derive tests before merging.
- One component per handoff, enforced with `--check-scope <component> --changed <file|->` (path list such as `git diff --name-only <base>`; `--allow <path>` for lockfiles or docs). Spec edits belong to the contract step; in a split spec a component's requirements and evidence live in its own fragment (pass it with `--allow spec/<component>.yaml`). A feature spanning components is delivered provider-first, each with its own slice, fakes and tests; a consumer never imports the provider in tests (`--check-drift`).
- Reopen the earliest affected gate and record it in the [gate decision record](CONSTITUTION.md#gate-decision-record); do not relabel an incomplete artifact to advance.
- A fake is only as good as its provider's contract tests; the change is not accepted until the provider verifies the behavior the fake models.
- The implementer returns a handoff return (`reference-implementation/handoff-return.schema.json`): `--check-return <file>` verifies every requirement of the component is a result or unmet, versions are current, failures are unmet, and changes stay in scope; its `results` map 1:1 to `Evidence` records for the component's fragment.
- Spec validation applies to every change. A project rule exempting docs- or spec-only changes from its full build must not exempt them from the spec gate: those are the changes the gate exists for.
- A project script that reports verification progress builds on `--evidence-report` instead of reimplementing the evidence evaluation; a reimplementation can disagree with the gate in both directions. Opt-in rules cover common policies: `system.evidence_policy.require_criteria` (`EV-08`) and `system.evidence_methods` per requirement `holder` (`EV-07`).
- A phased delivery declares `priority` on requirements and releases with `system.release_scope` (or `--release-scope`); deferred requirements are listed, never passed.
- Hidden coupling, undocumented failure modes or metamodel gaps found along the way go to [feedback](CONSTITUTION.md#self-improvement-and-model-feedback), not into a workaround.

## Decompose and hand off

Order: requirements and contracts first, then one independent implementation per component, then a join. The toolkit is tool-agnostic: the "implementer" below is whoever receives a slice (a person, a subagent or a separate session).

1. **Write the feature requirement** against the capability (steps 3-5), before any code.
2. **Split it** into sub-requirements that `refines` it, each targeting exactly one component (or a capability it provides). Each must be buildable and verifiable alone, against fakes.
3. **Order the work.** `--plan <id>` prints waves, providers first. Components in the same wave are independent and can be implemented in parallel; a later wave starts when the providers it `requires` are verified.
4. **Hand off** one component per implementer. The bundle is: the `--context <component-id>` slice, its sub-requirement, the fakes it uses, the allowed paths (`source_paths`/`test_paths`), and the [handoff return](reference-implementation/handoff-return.schema.json) to fill in.
5. **Check each return**: `--check-scope <component-id> --changed -` (nothing outside its paths) and `--check-return <file>` (every requirement of the component is a result or unmet).
6. **Join.** Record each `results` entry as evidence in the component's fragment. The parent requirement then needs its own integration evidence (`test_scope: integration` component), and every sub-requirement verified (`REQ-08`, since 0.5.0, unreleased). A failed join supersedes evidence and iterates; it does not relabel.

Illustration (not in the shipped example spec): feature "place an order reserves stock" is `req-order-reserves` on `commerce-system`. It is refined by `req-reserve` (target `reserve-inventory`, component `inventory`, wave 1) and `req-order` (target `place-order`, component `checkout`, wave 2, uses the `reserve_inventory` fake). Inventory and checkout implementers work from separate slices; the integration component proves `req-order-reserves` once both are verified and the provider's contract test keeps the fake honest.

Not specified: how an implementer is run (subagent, session, human), or how many run at once; that is project policy.

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

## Using with OpenSpec

[OpenSpec](https://github.com/Fission-AI/OpenSpec) (Fission AI) is a prose spec-and-change workflow for AI assistants: proposal, design notes, tasks, spec deltas (ADDED/MODIFIED/REMOVED), archive. It is a process layer; this model is the checked layer underneath. Neither requires the other, and the toolkit has no OpenSpec importer or exporter.

| OpenSpec | Here |
|---|---|
| Proposal, design notes | Scope and Contract steps, gate reviews |
| Spec delta | `--compat OLD.yaml` and `--impact <id>` |
| Task list | `--plan <id>` waves; one `--context <component>` slice per handoff |
| Archive | Version bumps, `supersedes` evidence history, [gate decision record](CONSTITUTION.md#gate-decision-record) |
| `openspec validate` | `--stage <stage>`, `--check-report` |

- Keep proposals and tasks in OpenSpec if the team uses it; the change is not accepted until the spec passes the Acceptance gate above. Archiving a change is not acceptance.
- Prose requirements and scenarios carry no contracts, components, data models or evidence: express them as `Requirement.criteria` and the model entities before relying on them.
- Treat the spec delta as a review aid; `--compat` is the check.
