# Toolkit Packaging Specification

Status: verified; implementation and offline export complete.
Machine-readable requirements: [`packaging.yaml`](packaging.yaml), validated at `verification` stage. Produced wheel and sdist verified via `uv build` and `tests/`.

## Outcome and scope

Distribute the validator, workflows, schema, examples and skills as one versioned Python toolkit. Users shall install a wheel, run `system-spec` outside this checkout, and export a readable toolkit directory without repository access.

- Retain distribution name `system-spec-ref`, current version `0.1.0` and Python requirement `>=3.12` from [`reference-implementation/pyproject.toml`](reference-implementation/pyproject.toml).
- Produce a pure-Python wheel and source distribution from the same revision and version. Registry publication is a separate delivery action.
- Runtime dependencies remain Pydantic and PyYAML; pytest and build tools are development dependencies.
- TLA+/TLC, Alloy and Modelable remain separately installed tools. Include their guides and example models; do not imply they are installed or executed by this package.
- A documentation ZIP may be added later from the same resource inventory. It is outside first-release acceptance.

## Package and resource layout

Use a root `pyproject.toml` as the authoritative distribution configuration, an import package named `system_spec_ref`, and a `system-spec` console entry point. The build backend is an implementation choice; it must support the standard wheel/source distribution workflow and explicit resource inclusion.

The following is a **proposed installed layout**, not existing repository paths:

```text
system_spec_ref/
  __init__.py
  metamodel.py
  cli.py
  toolkit/
    README.md
    CONSTITUTION.md
    GREENFIELD.md
    BROWNFIELD.md
    spec.md
    PLAN.md
    PACKAGING.md
    packaging.yaml
    .github/skills/{tla,alloy,modelable}/SKILL.md
    .github/workflows/metamodel.yml
    examples/...
    archive/...
    reference-implementation/...
```

`toolkit/` is an offline release snapshot retaining repository-relative paths. Its inclusion inventory shall be explicit: the listed root files, all currently tracked files under `examples/` and `archive/`, the three named skills, the metamodel workflow, and the currently tracked reference-implementation files (including its README, schema, example, old spec, corpus, formal model, tests, build files and lockfile). New files require an inventory decision; a recursive copy of the working directory is insufficient.

- Include hidden tracked support files in that inventory; exclude Git internals, environments, caches, generated build outputs and unrelated local files.
- Generate the runtime module and snapshot copy from the same authoritative source; do not maintain duplicate validator implementations manually.
- Ship the MIT `LICENSE` in the wheel, sdist and `toolkit/`; metadata declares `license = "MIT"`.
- Ship `WIRING.md` in `toolkit/`: export, project setup, agent-instruction snippet, explicit skill installation per agent, and upgrade steps. `system-spec export` prints a pointer to it.
- Publication runs only from `.github/workflows/publish.yml` on a GitHub release (tests, build, PyPI trusted publishing via the `pypi` environment); no CLI command publishes.
- Read installed resources through Python's package-resource API, independent of the working directory. Do not write into the installation directory.
- Preserve UTF-8 file content and relative paths during export. Hidden `.github` directories are required.
- Keep generated `schema.json` semantically equal to the installed validator's `--schema` output.

## CLI contract

These are proposed installed commands:

```text
system-spec [SPEC] [existing validator options]
system-spec --version
system-spec export DESTINATION
```

- Existing validation options retain their behavior: `--stage`, `--compat`, `--impact`, `--context`, `--schema`, `--json`, and help.
- With no specification argument, validate the bundled `reference-implementation/example.yaml`; never resolve that default against the caller's directory.
- Explicit specification and compatibility paths resolve against the caller's working directory, as today.
- `export` is reserved only as the first positional token. A specification literally named `export` remains accessible as `./export` or an absolute path.
- `--version` prints the installed distribution version and exits successfully.
- Preserve current validation results, JSON shapes and exit behavior: successful validation/context/impact/schema operations return 0; schema/metadata/unknown-ID failures return 1; invalid CLI syntax returns 2. Export operational failures return 1 with an actionable diagnostic on stderr.
- No command fetches remote resources, installs dependencies, activates skills, modifies agent configuration or publishes artifacts.
- Keep `python metamodel.py ...` usable from `reference-implementation/` during migration, including existing checkout tests and Docker/remote workflow invocations.

## Export contract

`system-spec export DESTINATION` shall create an offline snapshot of exactly the bundled `toolkit/` contents.

- Relative destinations resolve against the caller's working directory. The parent directory must exist and be writable.
- The destination must not exist. Reject files, directories and symlinks, including dangling symlinks; do not offer overwrite/merge behavior in the first release.
- Validate resource paths before writing: reject absolute paths, parent traversal, symlinks and any entry escaping the staging directory.
- Stage in a private sibling directory, then publish without replacing an existing destination. If another exporter wins the same destination, fail without changing its files.
- On failure, leave no partial destination. Clean up only staging files created by this invocation; report any cleanup failure and its location.
- On success, report the resolved destination and toolkit version. Exported files remain usable after the Python package is uninstalled.
- Export installs no skills. Users explicitly select and install exported skills through their agent's supported mechanism.
- Upgrades export to a new directory; merging user edits and migration tooling are deferred.

## Version and source compatibility

- Use one distribution version for code and bundled resources. Toolkit releases and modeled systems' `system.version` values are separate identities.
- Adding packaging must not change the metamodel, schema, stages or capability-version matching rules.
- Keep the resource layout stable within a major toolkit version. Review changes to CLI behavior, exported paths or the schema for compatibility and document migrations before release.
- Tests may inspect source-relative files when testing the checkout, but installed-artifact acceptance shall never depend on that layout or skip required checks because repository files are missing.

## Acceptance

All checks below are pending implementation. [`packaging.yaml`](packaging.yaml) supplies requirement IDs and criteria; no execution evidence is asserted.

| Requirement | Required verification |
|---|---|
| PKG-01 | Build wheel and source distribution from one clean revision; compare normalized resource inventories and versions. Build the wheel again from the unpacked source distribution. |
| PKG-02 | Install each wheel in a fresh environment outside the checkout. Run default validation and explicit valid/invalid inputs; compare existing options, JSON results and exit codes with the source CLI. |
| PKG-03 | Inspect package metadata: Python floor, runtime/development dependency separation, console entry point and consistent version. |
| PKG-04 | Compare bundled schema with installed `--schema`; parse exported YAML and preserve UTF-8 bytes on export. |
| PKG-05 | Export and validate the default example plus every greenfield/brownfield specification at its documented stage, including brownfield compatibility. Audit relative links in current guides and skills; record historical archive exceptions explicitly. |
| PKG-06 | Race two exporters for one absent target: exactly one succeeds and its complete tree remains unchanged by the loser. |
| PKG-07 | Reject traversal/absolute/symlink resource entries and existing destinations; prove files outside the new target/staging area remain unchanged. |
| PKG-08 | Inject copy and publication failures; prove no partial destination, useful stderr, nonzero exit and bounded staging cleanup. |
| PKG-09 | Install, invoke and export successfully on Windows and Linux with Python 3.12 and the latest stable Python supported by the dependencies. |
| PKG-10 | Confirm export copies each inventory entry once, with no network operations and no loading the entire toolkit into one in-memory archive; repeat with a synthetic inventory ten times larger. |
| PKG-11 | Measure CLI startup and peak memory against the unpackaged validator on the same host/interpreter; allow at most 20% regression with a 100 ms startup and 5 MiB memory allowance. |
| PKG-12 | Check version/success diagnostics and stderr failure reporting without leaking environment variables, credentials or input specification contents. |

Before release, run the existing pytest suite and documented example validations unchanged, then the installed-artifact checks above. Build/install/export success does not establish semantic correctness of arbitrary user specifications or execute external formal tools.

## Implementation order and feedback

1. Add failing artifact/CLI tests, root build metadata and importable validator; retain the source entry point and existing workflow behavior.
2. Bundle the explicit resource inventory, implement safe export and version reporting, then add isolated installed-artifact acceptance.
3. Update quick starts to installed commands after acceptance passes. Record the source revision, artifact digests, platforms, interpreter/dependency versions and actual results before a release decision.

Feedback under [constitution self-improvement](CONSTITUTION.md#self-improvement-and-model-feedback): skill links traverse `../../../`, the CLI default uses `__file__`, and existing tests inspect the checkout. These are physical artifact dependencies absent from the current metamodel's executable checks. Track them with PKG-02/04/05 rather than changing the schema or claiming `VALID` verifies resource closure, safe filesystem writes or distribution correctness. No metamodel extension is needed for this specification; revisit artifact bindings if these checks need reusable machine-enforced rules.
