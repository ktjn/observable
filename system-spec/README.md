# AI-Native System Specification

Contract-first metamodel, development workflows, and reference tooling for designing **greenfield** systems and onboarding **brownfield** systems with bounded AI context.

## Overview

Implementations are not the source of truth — capabilities, invariants, contracts, data models, verification criteria, and formal models are. Each AI or engineering task operates on a bounded component slice (`--context`) validated across progressive lifecycle stages (`--stage`).

```text
Capability → Contract → Data Model → Verification → Implementation → Evidence
```

## Repository Map

- **Core Rules & Metamodel**
  - [`CONSTITUTION.md`](CONSTITUTION.md) — shared principles, artifact rules, 10 verification categories, gate decision records, implementation handoffs, and change/reverification loop.
  - [`spec.md`](spec.md) — normative metamodel, 64 stage/compatibility invariants (§ 11), and reference serialization format.
- **Development Tracks**
  - [`GREENFIELD.md`](GREENFIELD.md) — 12-phase, 4-gate workflow (`discovery` → `architecture` → `verification` → `release`), AI Execution Rule, and prompts for new systems.
  - [`BROWNFIELD.md`](BROWNFIELD.md) — 4 adoption levels (`inventory` → `architecture` → `verification` → `release`), 5-step reverse-discovery protocol, risk waivers, and strangler migration prompts for existing systems.
- **Worked Examples**
  - [`examples/greenfield/`](examples/greenfield/README.md) — end-to-end commerce checkout & inventory walkthrough ([`spec.yaml`](examples/greenfield/spec.yaml), [`Inventory.tla`](examples/greenfield/Inventory.tla), [`Inventory.als`](examples/greenfield/Inventory.als)).
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
  - [`PACKAGING.md`](PACKAGING.md) — wheel, CLI, offline toolkit export and release acceptance ([machine-readable requirements](packaging.yaml)); PyPI readme in [`PYPI.md`](PYPI.md).
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

# Check declared source/schema files and hashes, then Python import and JSON Schema drift
uv run python metamodel.py spec.yaml --check-artifacts --root <repository>
uv run python metamodel.py spec.yaml --check-drift --root <repository>

# Export JSON Schema and run the full test suite
uv run python metamodel.py --schema > schema.json
uv run pytest
```

## License

[MIT](LICENSE)
