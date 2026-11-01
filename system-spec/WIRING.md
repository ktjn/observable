# Wiring System Spec into a Project

Offline toolkit snapshot. Nothing here is installed or activated automatically.

## 1. Export

```bash
pip install system-spec-ref
system-spec export ./system-spec     # destination must not exist
system-spec path/to/spec.yaml --stage architecture
```

* Exported copy works without the package installed (`python reference-implementation/metamodel.py ...` needs `pydantic`, `pyyaml`).
* Commit the export, or keep it outside the repo and reference it by path.

## 2. Add specs to your repo

* Create `system-spec/spec.yaml` in your project; start from `examples/greenfield/spec.yaml` or `examples/brownfield/level0.yaml`.
* Validate: `system-spec system-spec/spec.yaml --stage <inventory|discovery|architecture|verification|release>`.
* Greenfield: follow `GREENFIELD.md`. Brownfield: follow `BROWNFIELD.md` (adoption levels 0–3).
* Machine-readable format: `reference-implementation/schema.json` (also `system-spec --schema`).

## 3. Wire into agent instructions

Add to `AGENTS.md` (Codex, Copilot, and others) and/or `CLAUDE.md`:

```markdown
* system specification lives in `system-spec/`; follow `system-spec/CONSTITUTION.md` plus one track (`GREENFIELD.md` or `BROWNFIELD.md`)
* specs must validate with `system-spec <spec> --stage <stage>` before committing
* for bounded context use `system-spec <spec> --context <component-id>`
* report model gaps and hidden coupling as feedback (`CONSTITUTION.md` § Self-Improvement and Model Feedback)
```

## 4. Install skills (explicit, optional)

Guides for `tla`, `alloy`, `modelable` are in `.github/skills/<name>/SKILL.md`. Install per agent:

* GitHub Copilot: copy to your repo's `.github/skills/<name>/`.
* Claude Code: copy to `.claude/skills/<name>/` (project) or `~/.claude/skills/<name>/` (user).
* Other agents: point the instructions file at the `SKILL.md` path.
* Skill links use `../../../` to reach the toolkit docs; keep the exported tree intact or fix those links after copying.
* TLA+/TLC, Alloy and Modelable are separate tools and are not installed by this package.

## 5. Upgrade

* `pip install -U system-spec-ref`, then `system-spec export ./system-spec-new` (never overwrites).
* Diff against the old export, merge wanted changes, update the paths in your agent instructions and skills.
* Re-validate every spec at its stage; if validation changed, follow the migration notes in the release.
* Toolkit version (`system-spec --version`) is separate from a modeled system's `system.version`.
