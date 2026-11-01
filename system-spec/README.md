# AI-Native System Specification

Contract-first metamodel, development workflows, and reference tooling for designing **greenfield** systems and onboarding **brownfield** systems with bounded AI context.

## Overview

Implementations are not the source of truth — capabilities, invariants, contracts, data models, verification criteria, and formal models are. Each AI or engineering task operates on a bounded component slice (`--context`) validated across progressive lifecycle stages (`--stage`).

```text
Capability → Contract → Data Model → Verification → Implementation → Evidence
```

## Standalone Components

A component is the unit of work: it should be buildable and testable from its own `--context` slice, so an agent needs that slice, not the whole repository. The model supports this with explicit boundaries:

- `source_paths` and `test_paths` bind a component to its code and its own tests; test evidence must come from `test_paths` (`COMP-08`).
- Dependencies exist only through declared `requires` capabilities with interface contracts; `--check-drift` flags Python and JS/TS imports (source and tests) that bypass them.
- Each required capability has a fake under `test_paths` (`fakes`, `COMP-09`), and tests may not import other components (`--check-drift`), so tests run against fakes only; `test_scope: integration` exempts assembly/end-to-end tests, which may use providers of required capabilities.
- Platforms and third-party APIs are `external` components (`COMP-11`): contracts only, drawn outside the system; runtime-mediated dependencies name their mediator in `via`.
- A faked capability needs a verified `contract` requirement at its provider (`COMP-10`).
- Not enforced: that a fake matches the provider's contract tests, or that a slice alone suffices to build the component. See [`examples/components/`](examples/components/README.md).

| Check | Command | Verifies | Does not verify |
|---|---|---|---|
| Metadata | `system-spec spec.yaml --stage <stage>` | Invariants for the stage, incl. `COMP-08` | Any file on disk |
| Artifacts | `--check-artifacts --root <repo>` | Declared source/test/schema paths exist, stay inside the root, and hashes match | Content correctness |
| Drift | `--check-drift --root <repo>` | Python and JS/TS imports vs `requires`/`bindings`; tests importing other components; JSON Schema properties vs model fields | Other languages/formats, field types |
| Compatibility | `--compat OLD.yaml` | Capability and data-model changes vs the old spec | Clause semantics |
| Plan | `--plan ID` | Provider-first waves of affected components, their bindings, requirements and evidence to revisit (no validation) | That the plan is sufficient |
| Export | `--export backstage\|structurizr` | One-way catalog / C4 workspace output from the spec | Round-tripping; ownership (Backstage `--owner` is a parameter) |
| Return | `--check-return FILE` | An implementer's handoff return covers every requirement of the component, versions are current, failures are unmet, changes stay in scope | That results are true or tests meaningful |
| Scope | `--check-scope COMPONENT --changed FILE\|-` | Changed paths stay inside the component's source/test/fake/owned-schema paths (path comparison only; `--allow` extras) | File contents, selector-based tests |

## Repository Map

- **Core Rules & Metamodel**
  - [`CONSTITUTION.md`](CONSTITUTION.md) — shared principles, artifact rules, 10 verification categories, gate decision records, implementation handoffs, and change/reverification loop.
  - [`spec.md`](spec.md) — normative metamodel, 73 stage/compatibility invariants (§ 11), and reference serialization format.
- **Development Tracks**
  - [`GREENFIELD.md`](GREENFIELD.md) — 12-phase, 4-gate workflow (`discovery` → `architecture` → `verification` → `release`), AI Execution Rule, and prompts for new systems.
  - [`BROWNFIELD.md`](BROWNFIELD.md) — 4 adoption levels (`inventory` → `architecture` → `verification` → `release`), 5-step reverse-discovery protocol, risk waivers, and strangler migration prompts for existing systems.
  - [`FEATURE.md`](FEATURE.md) — 10-step workflow for adding or changing behavior in an already-specified system: impact, compatibility, per-component handoffs with fakes and tests, re-verification.
- **Worked Examples**
  - [`examples/greenfield/`](examples/greenfield/README.md) — end-to-end commerce checkout & inventory walkthrough ([`spec.yaml`](examples/greenfield/spec.yaml), [`Inventory.tla`](examples/greenfield/Inventory.tla), [`Inventory.als`](examples/greenfield/Inventory.als)).
  - [`examples/components/`](examples/components/README.md) — standalone components with their own source, tests, schema and drift/`COMP-08` checks.
  - [`examples/brownfield/`](examples/brownfield/README.md) — legacy billing progression from Level 0 to Level 3 (`level0.yaml` → `level1.yaml` → `old.yaml` → `spec.yaml`), including failed-gate recovery and `--compat` verification.
- **Reference Implementation & Formalization**
  - [`reference-implementation/`](reference-implementation/README.md) — validator ([`metamodel.py`](reference-implementation/metamodel.py)), JSON Schema ([`schema.json`](reference-implementation/schema.json)), Alloy metamodel ([`metamodel.als`](reference-implementation/metamodel.als)), and invalid test corpus ([`corpus.yaml`](reference-implementation/corpus.yaml)).
- **Skills**
  - [`.github/skills/tla/SKILL.md`](.github/skills/tla/SKILL.md) — TLA+/TLC concurrency and protocol verification.
  - [`.github/skills/alloy/SKILL.md`](.github/skills/alloy/SKILL.md) — Alloy structural and relational invariant verification.
  - [`.github/skills/modelable/SKILL.md`](.github/skills/modelable/SKILL.md) — deriving `.mdl` data contracts, typed codegen, and field-level compatibility checks.
- **Distribution & Adoption**
  - [`WIRING.md`](WIRING.md) — connecting an exported toolkit to a project, agent instructions, skills, CI and upgrades.
  - [`templates/BROWNFIELD-INTAKE.md`](templates/BROWNFIELD-INTAKE.md) — intake and gate template for onboarding an existing system.
  - [`PACKAGING.md`](PACKAGING.md) — wheel, CLI, offline toolkit export and release acceptance ([machine-readable requirements](packaging.yaml)); PyPI readme in [`PYPI.md`](https://github.com/ktjn/system-specification/blob/main/PYPI.md).
- **Roadmap & Archive**
  - [`PLAN.md`](PLAN.md) — delivered architecture, completed milestones, and optional next horizons.
  - [`archive/`](archive/README.md) — historical [review log](archive/REVIEW.md), [standards benchmark](archive/BENCHMARK.md) and onboarding [feedback](archive/feedback/).

## Install

```bash
pip install system-spec-ref
system-spec --version
system-spec spec.yaml --stage architecture   # same options as metamodel.py
system-spec export ./system-spec             # offline docs, examples, skills; then read WIRING.md
```

## Quick Start (from a checkout)

Run from [`reference-implementation/`](reference-implementation/README.md) using `uv` (or Python 3.12+ with `pydantic`, `pyyaml`, `pytest`):

```bash
# Validate a specification at a lifecycle stage (default: release)
uv run python metamodel.py ../examples/greenfield/spec.yaml --stage release

# Validate brownfield onboarding stages with active waivers
uv run python metamodel.py ../examples/brownfield/level0.yaml --stage inventory
uv run python metamodel.py ../examples/brownfield/level1.yaml --stage architecture
uv run python metamodel.py ../examples/brownfield/old.yaml --stage verification

# Check backward compatibility of capabilities and data models
uv run python metamodel.py ../examples/brownfield/spec.yaml --compat ../examples/brownfield/old.yaml

# Compute transitive change impact
uv run python metamodel.py example.yaml --impact reserve-inventory

# Export a bounded component slice for an AI implementation task
uv run python metamodel.py example.yaml --context checkout

# Render a Mermaid diagram of the system
uv run python metamodel.py example.yaml --diagram

# Check declared source/schema files and hashes, then Python/JS/TS import and JSON Schema drift
uv run python metamodel.py spec.yaml --check-artifacts --root <repository>
uv run python metamodel.py spec.yaml --check-drift --root <repository>

# Export JSON Schema and run the full test suite
uv run python metamodel.py --schema > schema.json
uv run pytest
```

## License

[MIT](LICENSE)
