# Wiring System Spec into a Project

Offline toolkit snapshot. Nothing here is installed or activated automatically.

## 1. Export

```bash
uv tool install system-spec-ref      # or: pip install system-spec-ref
system-spec export ./system-spec     # destination must not exist
system-spec path/to/spec.yaml --stage architecture
```

* Project-local alternative: `uv add --dev system-spec-ref`, then `uv run system-spec ...`.

* Exported copy works without the package installed (`python reference-implementation/metamodel.py ...` needs `pydantic`, `pyyaml`).
* Commit the export, or keep it outside the repo and reference it by path.
* For a reproducible adoption, pin the distribution version and record the index, wheel SHA-256, exported validator/schema and file hashes. If Git conversion is enabled, preserve the export bytes with scoped attributes and verify a checkout round trip.
* Standalone exported `--version` can report `0.0.0+unknown` without installed distribution metadata. Use the recorded package/wheel/export hashes to identify that snapshot.

## 2. Add specs to your repo

* For a new adoption, create a project spec (for example `system-spec/spec.yaml`); start from `examples/greenfield/spec.yaml` or `examples/brownfield/level0.yaml`.
* For an existing adoption, reuse its authoritative spec paths and gate records. The example directory is not mandatory; do not create a second baseline merely to match it.
* Validate: `system-spec system-spec/spec.yaml --stage <inventory|discovery|architecture|verification|release>`.
* Greenfield: follow `GREENFIELD.md`. Brownfield: follow `BROWNFIELD.md` (adoption levels 0–3).
* Machine-readable format: `reference-implementation/schema.json` (also `system-spec --schema`).
* Editor validation/autocomplete (YAML Language Server: VS Code, JetBrains, Cursor): first line of the spec, path relative to the spec file:

```yaml
# yaml-language-server: $schema=../system-spec/reference-implementation/schema.json
```

* Artifact paths (`source_paths`, `schema_path`, evidence `artifacts`/`provenance`, `subject_bindings`) resolve against the repository root given by `--root`, not the spec file's location. `--root` is required for `--check-artifacts` and `--check-drift`:

```bash
system-spec spec/system.yaml --check-artifacts --root .
system-spec spec/system.yaml --check-drift --root .        # Python imports, JSON Schema fields
```

## 3. CI gate

```yaml
- name: Validate system specification
  run: |
    pip install system-spec-ref
    system-spec spec/system.yaml --stage release
```

* Exit code is 0 for `VALID`, 1 for validation errors, 2 for invalid CLI usage.
* Pre-commit: a local hook running `system-spec spec/system.yaml --stage release` works the same way.

## 4. Wire into agent instructions

Add to `AGENTS.md` (Codex, Copilot, and others) and/or `CLAUDE.md`; substitute the actual project-spec and toolkit paths below:

```markdown
* system specification lives in `system-spec/`; follow `system-spec/CONSTITUTION.md` plus one track (`GREENFIELD.md` or `BROWNFIELD.md`)
* specs must validate with `system-spec <spec> --stage <stage>` before committing
* for bounded context use `system-spec <spec> --context <component-id>`
* report model gaps and hidden coupling as feedback (`CONSTITUTION.md` § Self-Improvement and Model Feedback)
```

## 5. Install skills (explicit, optional)

Guides for `tla`, `alloy`, `modelable` are in `.github/skills/<name>/SKILL.md`. Install per agent:

* GitHub Copilot: copy to your repo's `.github/skills/<name>/`.
* Claude Code: copy to `.claude/skills/<name>/` (project) or `~/.claude/skills/<name>/` (user).
* Other agents: point the instructions file at the `SKILL.md` path.
* Skill links use `../../../` to reach the toolkit docs. Copied into your repo's `.github/skills/<name>/`, they must point at the export instead: replace `../../../` with `../../../system-spec/` (adjust to your export directory), for example:

```bash
# run once
sed -i 's#\.\./\.\./\.\./#../../../system-spec/#g' .github/skills/*/SKILL.md
```

* Alternatively leave the skills in the export and point your agent's instructions at `system-spec/.github/skills/<name>/SKILL.md`; no rewrite is needed.
* TLA+/TLC, Alloy and Modelable are separate tools and are not installed by this package.

## 6. Upgrade

* `pip install -U system-spec-ref`, then `system-spec export ./system-spec-new` (never overwrites).
* Diff against the old export, merge wanted changes, update the paths in your agent instructions and skills.
* Re-validate every spec at its existing stage, including artifact checks where used; if validation changed, follow the migration notes in the release.
* Preserve historical validator/source/evidence pins and decisions rather than relabeling old captures. Record the new snapshot and review schema/validation differences before reusing evidence.
* Installation or successful metadata validation does not advance adoption levels, approve a contract proposal or replace native/manual acceptance gates.
* Toolkit version (`system-spec --version`) is separate from a modeled system's `system.version`.
