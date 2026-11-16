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
* The schemas (`schema.json`, `fragment.schema.json`, `handoff-return.schema.json`) are LF files byte-identical to `system-spec --schema`, `--fragment-schema` and `--return-schema` output, so one SHA-256 identifies both (releases before the next one shipped CRLF files, whose hash differs from the command's output; compare parsed JSON for those).
* Standalone exported `--version` can report `0.0.0+unknown` without installed distribution metadata. Use the recorded package/wheel/export hashes to identify that snapshot.

## 2. Add specs to your repo

* For a new adoption, create a project spec (for example `system-spec/spec.yaml`); start from `examples/greenfield/spec.yaml` or `examples/brownfield/level0.yaml`.
* For an existing adoption, reuse its authoritative spec paths and gate records. The example directory is not mandatory; do not create a second baseline merely to match it.
* Validate: `system-spec system-spec/spec.yaml --stage <inventory|discovery|architecture|verification|release>`.
* Greenfield: follow `GREENFIELD.md`. Brownfield: follow `BROWNFIELD.md` (adoption levels 0–3).
* Large systems: split the spec so agents edit separate files. Keep `system` in the root with `capabilities: []`, `components: []` and `include: [spec/*.yaml]`; put each component's capability, component, data models, requirements and evidence in `spec/<component>.yaml` (first line `# yaml-language-server: $schema=.../reference-implementation/fragment.schema.json`). See `examples/components/`.
* Machine-readable format: `reference-implementation/schema.json` (also `system-spec --schema`). A project that only installs the package has no such file to point `$schema` at: run `system-spec --schema > system-spec/schema.json` (`--fragment-schema > system-spec/fragment.schema.json` for fragments), commit it, start the spec with `# yaml-language-server: $schema=schema.json` (path relative to the spec file) and regenerate it when upgrading the package; its hash is the pin.
* Editor validation/autocomplete (YAML Language Server: VS Code, JetBrains, Cursor): first line of the spec, path relative to the spec file:

```yaml
# yaml-language-server: $schema=../system-spec/reference-implementation/schema.json
```

* Bind each component to its code and tests (`source_paths`, `test_paths`) so it can be developed from its `--context` slice; see `examples/components/`. Test evidence provenance should name the actual test file (optionally with `::selector`) under `test_paths`; keep JUnit/output captures in evidence `artifacts`. Unit-scoped components need a fake (`fakes`) under `test_paths` for each required capability. Use `test_scope: integration` only for observed assembly/end-to-end tests; it permits declared providers rather than claiming isolation.
* Artifact paths (`source_paths`, `schema_path`, evidence `artifacts`/`provenance`, `subject_bindings`) resolve against the repository root given by `--root`, not the spec file's location. `--root` is required for `--check-artifacts` and `--check-drift`:

```bash
system-spec spec/system.yaml --check-artifacts --root .
system-spec spec/system.yaml --check-drift --root .        # Python/JS/TS imports, JSON Schema fields
```

* `--check-artifacts`, `--check-drift`, `--check-imports` and `--check-isolation` run the stage checks too when `--stage` or `--compat` is given explicitly, and report all errors together. Without an explicit `--stage`, drift, imports and isolation run alone; validators before this change skipped the stage silently even when `--stage` was given, so a combined command printed `VALID` for a spec that failed the stage.
* Rust with cargo-nextest: its JUnit names no file (`classname` is the binary ID: `pkg` for the library's unit tests, `pkg::name` for an integration test, `pkg::bin/name`), so resolve it with the adapter instead of writing your own:

```bash
cargo metadata --format-version 1 --no-deps > meta.json
system-spec spec/system.yaml --emit-test-map junit.xml --cargo-metadata meta.json --root . > test-map.yaml
# several runs (a crate outside the workspace has its own metadata and report): one --cargo-metadata per --emit-test-map, in order
system-spec spec/system.yaml --check-report junit.xml --test-map test-map.yaml --report-scope evidence --root .
```

  The map gives each test `<file>::<path within the file>::<fn>` (target source file, then the `mod` declarations along the test's module path, `#[path]` included; the modules the file itself stands for are dropped, so `src/store.rs::tests::round_trips`, as `--check-test-names` expects; macro-generated or cfg-gated modules are reported as unresolved) and the requirements of evidence that cites the file, the exact test or the `nextest::-p PKG` selector covering it. Paths are relative to `--root` (default: the current directory) whichever workspace a report came from.
* nextest writes its JUnit to `<target dir>/nextest/<profile>/junit.xml` (file name from `[profile.<name>.junit] path`), where the target dir is the one `cargo metadata` reports for the manifest it ran on; `--target-dir` does not move it. A crate outside the workspace run with `--manifest-path X` therefore has its report in `X`'s own `target/nextest/...`. When `CARGO_TARGET_DIR` is set, the workspace and every such crate share one store and their `junit.xml` overwrite each other: copy each report out after its run, or give each crate its own `CARGO_TARGET_DIR`.
* Drift resolves absolute Python imports only under the repository root or `root/src`; other package roots (for example `cli/src` in a monorepo) can be missed without an `unchecked` entry. A clean drift result is not a complete dependency map.

## 3. CI gate

```yaml
- name: Validate system specification
  run: |
    pip install system-spec-ref
    system-spec spec/system.yaml --stage release
```

* Per-task scope gate for agent PRs: `git diff --name-only origin/main | system-spec spec/system.yaml --check-scope <component> --changed -` (add `--allow <path>` for shared files such as lockfiles).
* Agent handoff return gate: `system-spec spec/system.yaml --check-return handoff-return.yaml` (schema: `reference-implementation/handoff-return.schema.json`). A return with every obligation explicitly `unmet` can be structurally valid; inspect unmet work and findings before acceptance. System-targeted requirements and native/manual gates need separate review.
* Exit code is 0 for `VALID`, 1 for validation errors, 2 for invalid CLI usage.
* Pre-commit: a local hook running `system-spec spec/system.yaml --stage release` works the same way.

## 4. Wire into agent instructions

Add to `AGENTS.md` (Codex, Copilot, and others) and/or `CLAUDE.md`; substitute the actual project-spec and toolkit paths below:

```markdown
* system specification lives in `system-spec/`; follow `system-spec/CONSTITUTION.md` plus one track (`GREENFIELD.md` or `BROWNFIELD.md`; `FEATURE.md` for changes to an already-specified system; `DOMAIN-AGENTS.md` when a scope spans multiple domains)
* specs must validate with `system-spec <spec> --stage <stage>` before committing
* for bounded context use `system-spec <spec> --context <component-id>`
* report model gaps and hidden coupling as feedback (`CONSTITUTION.md` § Self-Improvement and Model Feedback); upstream reports carry no project specifics (`README.md` § Giving feedback)
```

## 5. Install skills (explicit, optional)

Guides for `tla`, `alloy`, `quint`, `lean`, `modelable` are in `.github/skills/<name>/SKILL.md`. Install per agent:

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
* Before adding `test_paths`, check retained test provenance against `COMP-08`. If a record names a log, preserve that artifact and its hashes, identify the actual executed test file from its capture, and record the provenance correction with the old spec/manifest retained. Do not add log directories to test paths or claim a rerun. Add criterion/environment metadata only where the retained execution supports it.
* Installation or successful metadata validation does not advance adoption levels, approve a contract proposal or replace native/manual acceptance gates.
* Toolkit version (`system-spec --version`) is separate from a modeled system's `system.version`.
