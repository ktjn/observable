# Repository Review: AI-Native System Specification

**Reviewer**: cjot  
**Date**: 2026-10-30 (Updated, rev. 17)
**Repository**: ktjn/system-specification

---

## Overview

- Repository spans principles (`CONSTITUTION.md`), track workflows (`GREENFIELD.md`, `BROWNFIELD.md`), a prose metamodel (`spec.md`) and a working **reference implementation** (`reference-implementation/`).
- Rev. 7: `metamodel.py` now enforces most `spec.md` §§ 2–11 invariants; compatibility, impact analysis and `--json` are wired into the CLI; a Docker-based `metamodel` workflow exists.
- Rev. 8: new `PLAN.md` roadmap (greenfield/brownfield lifecycle) reviewed.
- Rev. 9: `PLAN.md` revised per rev. 8 critical item 2; no code/spec changes since rev. 7.
- Rev. 10: docs restructured into shared constitution + greenfield/brownfield tracks; no code changes.
- Rev. 11: `spec.md` scoped to metamodel/format; prototype details moved to README; `tla` and `alloy` skills added (#10, #11); no validator changes.
- Rev. 12: `modelable` skill (#13) repositioned to implementation/version change; `example.mdl` removed (duplicated `data_models`); no validator changes.
- Rev. 13: rev. 8 validator issues fixed (#15–#18): unknown `mitigated_by`/`--impact` IDs, requirement cycles, `avoidance`/`transfer` treatments with residual risk tracking. CI still postponed.
- Rev. 14: stages, waivers, evidence, interface and lifecycle semantics specified in `spec.md` (8f40f6c) with greenfield/brownfield examples; validator not yet extended. Formalization review: `spec.md` is a prose metamodel; abstract syntax lives only in code.
- Rev. 15: validator at par with `spec.md` (#22): `extra="forbid"`, `--stage`, waivers, lifecycle, interfaces, budgets, categories, evidence supersession/`model-check`, `--context`, data-contract `--compat`; examples validate per stage.
- Rev. 16: formalization steps 1–3, 6 (#23–#25): committed `schema.json` with field descriptions, invariant IDs as data (`INVARIANTS` ↔ `spec.md` § 11 table), `[ID]`-prefixed errors, invalid corpus per ID.
- Rev. 17: usable development process (gate review decisions, implementation handoffs, change/reverification loop, walkthroughs with failed-gate recovery), formalization steps 4–5 (drift-checked format blocks against `schema.json`, Alloy metamodel `reference-implementation/metamodel.als`), sample formal models (`examples/greenfield/Inventory.tla`, `.cfg`, `.als`), track prompts citing `schema.json` and invariant IDs, and `spec.md` § 11 `CAP-01` alignment.

---

## Changes Since Last Review (rev. 16)

| Area | Change | Status |
|------|--------|--------|
| Process & Walkthroughs | Gate review decisions, implementation handoff, and change/reverification loop in `CONSTITUTION.md`, `GREENFIELD.md`, `BROWNFIELD.md`; end-to-end walkthroughs with Gate Decision Records and failed-gate recovery in `examples/greenfield/README.md` and `examples/brownfield/README.md` | ✅ Done |
| `spec.md`, `test_metamodel.py` | Completed **Current reference format** blocks across §§ 2–8; drift test `test_format_blocks_match_schema` (step 4); removed inaccurate `unless waived` on `CAP-01` in § 11 | ✅ Done |
| `reference-implementation/metamodel.als` | Alloy model of the metamodel checking `ReleaseValid`, `LineagePreservesClassificationWithoutDeclassification`, and `NoUnsupersededFailuresForMandatoryRequirements` (step 5) | ✅ Done |
| `examples/greenfield/Inventory.*` | Added sample `Inventory.tla`, `Inventory.cfg`, and `Inventory.als` formal models referenced by greenfield evidence | ✅ Done |
| `GREENFIELD.md`, `BROWNFIELD.md` | Track prompts reference `reference-implementation/schema.json` and cite § 11 invariant IDs; verified by `test_prompts_cite_invariants_and_schema` | ✅ Done |
| `.github/workflows/metamodel.yml` | Added `push` and `pull_request` triggers defaulting `SCRIPT` to `pytest` (runs all 91 tests incl. `example.yaml` and `examples/`) | ✅ Done |

---

## Changes in rev. 16 (since rev. 15)

| Area | Change | Status |
|------|--------|--------|
| `metamodel.py`, `schema.json` (#23) | Field descriptions citing `spec.md` §; committed JSON Schema (draft 2020-12); specs reference it via `yaml-language-server` modeline; drift test | ✅ Done |
| `metamodel.py`, `spec.md` § 11 (#24) | `INVARIANTS` table (ID → stage, waiver kind, rule) drives `--stage`; 57 IDs in 9 domains (`REF`, `DATA`, `EV`, `RISK`, `STAT`, `CAP`, `COMP`, `REQ`, `COMPAT`); errors prefixed `[ID]`; § 11 table replaces prose bullets; sync test | ✅ Done |
| `corpus.yaml`, tests (#25) | 61 invalid cases patching `example.yaml`, exact expected ID sets; coverage test per ID; 23 single-error tests migrated; 88 tests | ✅ Done |
| Docs (#23–#25) | `spec.md` § 12, README, AGENTS.md point to `schema.json`; PLAN Milestone 5 steps 1–3, 6 ticked | ✅ Done |

---

## Changes in rev. 15 (since rev. 14)

| Area | Change | Status |
|------|--------|--------|
| `metamodel.py` (#22) | `extra="forbid"`; `status`, `category`, `resolved_by`, `interface`, `context_budget`, `source_paths`, `schema_path`, `declassified_by`, `waives`/`expires`, evidence `model-check`/`bounds`/subject/`supersedes` | ✅ Done |
| `metamodel.py` (#22) | `--stage` (default `release`): each check tagged with a § 11 stage; waivable kinds matched by risk `target`; `--json` lists `stage`, `waivers` | ✅ Done |
| `metamodel.py` (#22) | `--context <component>` slice; `--compat` checks data contracts (fields, types, classification, major bump) | ✅ Done |
| Examples (#22) | Statuses, categories, interfaces, `model-check`; `example.yaml` covers 10 categories; brownfield `level0.yaml`/`level1.yaml`; `old.yaml` = Level 2 with category waiver | ✅ Done |
| Docs (#22) | "planned" markers removed (`spec.md`, CONSTITUTION, tracks, skills, example READMEs); § 11 `release` row: "all risks disposed"; PLAN Milestones 2–4 ticked | ✅ Done |

---

## Changes in rev. 14 (since rev. 12)

| Area | Change | Status |
|------|--------|--------|
| `metamodel.py` (#15) | Unknown `Risk.mitigated_by` IDs reported as unknown references | ✅ Done |
| `metamodel.py` (#16) | `--impact` exits 1 on unknown IDs | ✅ Done |
| `metamodel.py` (#17) | Requirement `depends_on`/`refines` cycles detected (reuses `_cycles`) | ✅ Done |
| `metamodel.py` (#18) | `avoidance` needs verified `mitigated_by`, no residual; `transfer` needs `transferred_to` + `residual_accepted`; `--json` lists `residual_risks` | ✅ Done |
| `spec.md`, README, `example.yaml` (#18) | `transferred_to` documented; transfer risk example | ✅ Done |
| `PLAN.md` (#19) | Milestone 1 validator item ticked | ✅ Done |
| `spec.md` (8f40f6c) | Entity `status`; 5-stage validation table (`status` per stage, invariants, waivers); `waives`/`expires`; context budget defaults (`5/8/4`); `interface` on capabilities; `source_paths`, `schema_path`, `declassified_by`; requirement `category`/`resolved_by`; evidence `model-check`, `bounds`, `subject_revision`, `supersedes` | ✅ Specified, impl. pending |
| `examples/greenfield/`, `examples/brownfield/` (8f40f6c) | Traced 12-phase/4-gate and Level 0→3 examples; all `VALID`; brownfield `--compat old.yaml` passes | ✅ Done |
| `reference-implementation/old.yaml` (8f40f6c) | Sample for `--compat` | ✅ Done |
| Tracks, skills (8f40f6c) | Gates/levels reference `spec.md` § 11 and link `tla`/`alloy`/`modelable`; skills note `model-check` | ✅ Done |
| `PLAN.md` | Formalization steps (§ 5) and Milestone 5 | ✅ Done |
| `REVIEW.md` | Formalization review added | ✅ Done |

---

## Changes in rev. 12 (since rev. 11)

| Area | Change | Status |
|------|--------|--------|
| `.github/skills/modelable` | Derives `.mdl` from `data_models` at implementation/version change: codegen, `diff`/`validate-compat` → `Evidence` | ✅ Done |
| `reference-implementation/example.mdl` | Added in #13, removed: duplicated `data_models` | ✅ Resolved |
| `example.yaml` | `req-order-contract`/`ev-order-contract` from #13 removed with `example.mdl` | ✅ Resolved |
| `CONSTITUTION.md` | `data_models` authoritative; links skill for implementation/version change | ✅ Done |

---

## Changes in rev. 11 (since rev. 10)

| Area | Change | Status |
|------|--------|--------|
| `spec.md` | Normative metamodel + current reference format only; § 12 lists required demonstrations | ✅ Done |
| `reference-implementation/README.md` | Validator behavior and limitations moved here from `spec.md` | ✅ Done |
| `.github/skills/tla` | TLA+/TLC how-to mapping contract clauses to properties and output to `Evidence` | ✅ Done |
| `.github/skills/alloy` | Alloy how-to for structural/relational invariants; points to `tla` for concurrency | ✅ Done |
| `CONSTITUTION.md` | Formal Verification links both skills with scope; capability format linked | ✅ Done |
| `PLAN.md` | Doc structure lists skills | ✅ Done |
| `example.yaml` | `req-projection` on `order-stock-projection` with Alloy evidence | ✅ Done |

---

## Changes in rev. 10 (since rev. 9)

| Area | Change | Status |
|------|--------|--------|
| `CONSTITUTION.md` | Replaces `AI-native-design.md`; English, order-free shared artifact rules; 10 verification categories (Security added) | ✅ Done |
| `GREENFIELD.md` | Replaces `AI-System-Development-Constitution.md`; phases → gates table, AI Execution Rule, prompts | ✅ Done |
| `BROWNFIELD.md` | New; adoption levels, reverse discovery, waivers, modernization, prompts; overrides greenfield ordering | ✅ Done |
| `PLAN.md` | Doc structure (§ 1.1); `PROMPTS.md`/`ONBOARDING.md` dropped; roadmap phases → milestones | ✅ Done |
| `spec.md` | Links updated | ✅ Done |

---

## Changes in rev. 9 (since rev. 8)

| Area | Change | Status |
|------|--------|--------|
| `PLAN.md` baseline | Phase 1 retargeted to rev. 8 remaining issues | ✅ Done |
| `PLAN.md` states | Single `status` set (`draft/specified/verified/deprecated`); adoption levels map to `--stage` | ✅ Done |
| `PLAN.md` reuse | Uses `bindings`, `--impact`, `--compat`, `metamodel.yml`; `provider_overrides` dropped | ✅ Done |
| `PLAN.md` CLI | Keeps flag style; adds `--stage`, `--context` | ✅ Done |
| `PLAN.md` coverage | Adds requirement `status`, evidence revision/supersession, cycle and unknown-ID fixes | ✅ Done |

---

## Changes in rev. 8 (since rev. 7)

| Area | Change | Status |
|------|--------|--------|
| `PLAN.md` | Added: metamodel extensions, greenfield gates, brownfield adoption levels, tooling, 4-phase roadmap | ✅ Added |

---

## Changes in rev. 7 (since rev. 6)

| Area | Change | Status |
|------|--------|--------|
| `metamodel.py` resolution | Ambiguous providers rejected; `bindings` disambiguate; self-dependency excluded; cycles detected | ✅ Done |
| `metamodel.py` § 11 | Duplicate IDs, ambiguous/unknown targets, dangling references, declared layers | ✅ Done |
| `metamodel.py` § 5 | Bidirectional ownership; unknown lineage reported separately from classification violation | ✅ Done |
| `metamodel.py` §§ 7–8 | Failing evidence blocks verification; mitigations require verified requirements | ✅ Done |
| `metamodel.py` § 10 | `check_compatibility` covers failure modes + major bump; `compare_specs` handles removal/deprecation; `--compat` CLI | ✅ Done |
| `metamodel.py` impact | Transitive; includes data models, system-level requirements, dependent requirements, invalidated evidence | ✅ Done |
| `metamodel.py` versions | Non-numeric versions no longer crash | ✅ Done |
| CLI | `--json`, `--impact`, `--compat`; schema vs. metadata errors distinguished | ✅ Done |
| Tests | 29 tests (up from 15), incl. CLI | ✅ Done |
| Tooling | `Dockerfile` + manually dispatched `metamodel` workflow | 🟡 Partial |
| `AGENTS.md` | Validation-before-commit, `--schema`, remote workflow rules | ✅ Done |
| Constitution | Phase 6 explicitly mapped (no API-specific entity) | ✅ Done |

---

## Status of Previous Recommendations

| # | Recommendation | Status | Notes |
|---|----------------|--------|-------|
| 1 | Verification evidence format | ✅ Done | `model-check`/`bounds`, subject fields, `supersedes` validated; subject fields optional |
| 2 | Phase → metamodel mapping | ✅ Done | |
| 3 | Prompt templates | ✅ Done | Prompts in `GREENFIELD.md`/`BROWNFIELD.md` cite `schema.json` and § 11 invariant IDs; drift-tested |
| 4 | Capability contract format | ✅ Done | |
| 5 | Versioning & compatibility | ✅ Done | `--compat` checks capabilities and data contracts; field constraints via `modelable` |
| 6 | Data lineage / classification | ✅ Done | `declassified_by` relaxes propagation; requires evidence at `release` |
| 7 | Composition algorithm | ✅ Done | |
| 8 | Requirement dependency model | ✅ Done | Cycles, `status` (inactive `deprecated`), `resolved_by` |
| 9 | Layering invariant | ✅ Done | |
| 10 | Worked example | ✅ Done | `examples/greenfield/`, `examples/brownfield/` with Gate Decision Records and failed-gate recovery |
| 11 | Reference implementation roadmap | ✅ Done | All `PLAN.md` milestones and automatic CI (`push`/`pull_request`) complete |

---

## Reference Implementation Review

### Strengths ✅

- Compact (~490 lines incl. invariant table) and readable; each check keyed by invariant ID.
- Covers all specified fields and the five § 11 stages, incl. waivers; unknown fields rejected.
- Rev. 8 issues fixed: unknown references, requirement cycles, all four risk treatments checked.
- All rev. 6 bugs fixed and covered by tests (91 tests total).
- Clear separation of schema errors (Pydantic) and metadata errors.
- `README.md` honestly lists validator behavior, limits and what `VALID` means.

### Remaining Issues 🐞

- `source_paths`, `schema_path`, evidence `subject_revision`/`timestamp`/`verifier` parsed only; subject fields optional at `release`.
- Category ↔ `kind` consistency (CONSTITUTION table) unchecked.
- `consistency`, `quality_attributes` parsed but unverified (documented).

### Tooling

- `metamodel` workflow runs automatically on `push` and `pull_request` (`pytest` + all example specs) and supports `workflow_dispatch`.
- Invalid corpus is one YAML file of patches (`corpus.yaml`), not standalone specs; fine for tests, less so as LLM few-shot material.

---

## Skills Review

### Strengths ✅

- Compact, parallel structure (inputs, steps, evidence, prompt); one property per contract clause.
- Clear split: `tla` for interleaving/protocols, `alloy` for structure/relations.
- Both state that bounded checks are not unbounded proofs and require recording bounds/scope.
- `example.yaml` shows evidence from both (`tla+::`, `alloy::`), backed by `examples/greenfield/Inventory.tla`, `Inventory.cfg`, and `Inventory.als`.

### Remaining Issues 🐞

- **Evidence naming**: `ev-reserve-test` is a `model-check`, not a test (pre-existing ID).
- **`order_paid_at_most_once`**: concurrency invariant with no requirement or evidence in `example.yaml` or `examples/greenfield/`; natural `tla` candidate.
- **`modelable`**: verified against modelable 1.x, but needs Python 3.14 and a `selectolax<1` workaround; classification is field-level (`open`…`secret`) vs. model-level (`public`…`restricted`) in the metamodel.

### Data Contract Duplication ✅ Resolved

- `example.mdl` restated `example.yaml` `data_models` with no consistency check and forced fits (`on true` join, hand-set classification).
- At specification time Modelable mostly overlaps `metamodel.py` (ownership, lineage, classification); its unique value is codegen and field-level compatibility.
- Resolution: `data_models` stay authoritative; `.mdl` is a derived implementation artifact (greenfield phase 9, version changes); `example.mdl` deleted.

---

## PLAN.md Review

### Strengths ✅

- Covers both greenfield (phase gates) and brownfield (adoption levels, reverse discovery) adoption.
- Gates map onto greenfield phases; stage-aware validation fits the existing validator.
- Brownfield exceptions tied to `Risk` reuse existing entities instead of adding new ones.
- Absorbs open rev. 7 items: prompt templates, worked example, CI, invalid fixtures, conflict resolution, declassification.

### Resolved since rev. 8 ✅

- Doc sprawl (rev. 10): three docs, `PROMPTS.md`/`ONBOARDING.md` dropped.
- Stale baseline, CI duplication, re-implementing `--impact`/`--compat`, CLI shape, `provider_overrides`, inconsistent states, missing items.
- 8f40f6c: `status` ↔ stage table; `discovery` skip stated; Levels 1–2 pass via waivers (incl. missing categories at Level 2); waiver expiry = ISO date or stage; per-stage invariants; budget defaults `5/8/4`; `interface` as `Capability` extension.
- Validator hardening (#15–#18) done; formalization completed as Milestone 5 (steps 1–6).
- Rev. 14 issues (#22): § 11 risk wording fixed; Milestone 2 delivered; examples carry `status`, waivers and interfaces.
- Rev. 17: Usable Development Process items and Milestone 5 optional steps 4–5 completed.

---

## Formalization Review

`spec.md` is a prose metamodel ("shall" requirements + **Current reference format** blocks), backed by machine-checked schema, invariant tables, and an Alloy model (`reference-implementation/metamodel.als`).

| Layer | Defines | Lives in | Formal? |
|---|---|---|---|
| Concrete syntax | How a spec is written | YAML | ✅ (YAML standard) |
| Abstract syntax | Entities, fields, types | Pydantic models → committed `schema.json` (drift-checked against `spec.md` format blocks) | ✅ JSON Schema |
| Static semantics | Well-formedness invariants | `spec.md` § 11 ID table ↔ `INVARIANTS`; `metamodel.als`; `validate_spec()`; `corpus.yaml` | ✅ identified, modeled + tested |
| Meaning | What a valid spec says about the system | `spec.md` §§ 2–10 | ❌ prose |

### Proposed Steps

| # | Step | Human | LLM | Checker |
|---|---|---|---|---|
| 1 | Commit `schema.json` (from `--schema`, with field descriptions) | Editor completion via `$schema` | Structured output, valid by construction | Pydantic stays the parser |
| 2 | Stable invariant IDs in § 11 (e.g. `INV-REF-01`) | Citable rules | Explains and fixes cited errors | Errors/tests cite IDs; coverage test |
| 3 | Invariants as data (ID, stage, check, message) | One rule table | Stage-scoped rules | Drives `--stage` |
| 4 | Drift-check format blocks against schema | No drift | Same | One source of truth |
| 5 | Alloy model of the metamodel (`metamodel.als`) | Rules shown consistent | — | Independent cross-check |
| 6 | Valid/invalid example corpus | Learn by example | Few-shot examples | Per-invariant tests |

Done: 1–6.

---

## Gaps in spec.md

- None identified (format blocks drift-tested against `schema.json`; § 11 invariant table drift-tested against `INVARIANTS`; `CAP-01` wording aligned).

---

## Gaps in Other Documents

- **CONSTITUTION.md**: ✅ shared rules; artifact → metamodel mapping; gate decision records, implementation handoffs, and change/reverification loop.
- **GREENFIELD.md**: ✅ gates reference `spec.md` § 11 and skills; prompts cite `schema.json` and invariant IDs; walkthrough in `examples/greenfield/`.
- **BROWNFIELD.md**: ✅ waiver expiry/scope defined; prompts cite `schema.json` and invariant IDs; walkthrough in `examples/brownfield/`.
- **AGENTS.md**: ✅ validation rules; `schema.json` regenerated on model change.
- **reference-implementation/README.md**: ✅ validator behavior and limitations.
- **Skills**: ✅ linked from tracks; sample `Inventory.tla`, `Inventory.cfg`, `Inventory.als`, and `metamodel.als` committed.
- **examples/**: ✅ each file `VALID` at its stage (greenfield `release`; brownfield Levels 0–3).
- **PLAN.md**: ✅ Usable Development Process and Milestones 1–5 complete.

---

## Recommendations (Prioritized)

### 🟢 Medium

1. `source_paths`/`schema_path` drift checks; require evidence subject fields at `release`.
2. Waiver kinds as invariant IDs (`waives: COMP-05`) to drop the parallel kind vocabulary.

---

## Gaps Summary Table

| Document | Section | Gap | Priority |
|----------|---------|-----|----------|
| metamodel.py | bindings | `source_paths`/`schema_path` parsed only | 🟢 |

---

## Conclusion

- Validator hardening complete (#15–#18); all examples validate, `--compat` sample in place.
- Validator at par with `spec.md` (#22): stages, waivers, lifecycle, interfaces, budgets, categories, evidence supersession, `--context`, data-contract `--compat`; unknown fields rejected.
- Formalization steps 1–6 done (#23–#25, rev. 17): `schema.json`, invariant IDs as data, invalid corpus, format-block drift check, and `metamodel.als`.
- Usable Development Process and `PLAN.md` Milestones 1–5 completed (including automatic `push`/`pull_request` CI).

**Assessment**

- Metamodel clarity: ⭐⭐⭐⭐⭐
- Specification completeness: ⭐⭐⭐⭐⭐
- Reference implementation: ⭐⭐⭐⭐⭐ (at par with `spec.md`; unknown fields rejected; 91 tests)
- Implementation guidance for teams/AI: ⭐⭐⭐⭐⭐ (track docs with schema/invariant-citing prompts and end-to-end walkthroughs)
- Roadmap (`PLAN.md`): ⭐⭐⭐⭐⭐ (Milestones 1–5 and Usable Development Process complete)

---

**cjot** | 2026-10-30
