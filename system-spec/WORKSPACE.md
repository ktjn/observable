# Workspace: Distributed Specifications

Several independently owned specifications composed by a super-repo. Artifact rules: [constitution](CONSTITUTION.md). Format: [`spec.md` § 12](spec.md#12-reference-implementation).

## Model

- One repository owns one specification (a *member*). It is the only authority for its capabilities, data models and requirements.
- Other members reference it with `system.imports`; they never copy its contracts.
- The super-repo (*workspace*) owns no system. It holds the manifest, the member checkouts and cross-member CI.
- Members validate alone. The workspace adds only cross-member checks.

## Links, not revisions

- `SpecImport.link` is the owning repository's URL. It carries no revision.
- The pin is content: imported `system.version`, per-capability versions and optional `sha256`.
- Pin drift is detected by comparing the checked-out member to the pin.
- NOTE: a pin does not record which commit was reviewed. Reproducible checkouts (submodules, lockfile, CI clone step) are tooling, not model.
- NOTE: `sha256` covers the imported root file only, not its `include` fragments. Contract changes in fragments are caught by version pins; bump the capability version.

## Layout

```text
super-repo/
  workspace.yaml
  platform/spec.yaml    # clone of https://git.example/platform
  app/spec.yaml         # clone of https://git.example/app
```

```yaml
id: acme
members:
  - { id: platform, path: platform/spec.yaml, link: https://git.example/platform }
  - { id: app, path: app/spec.yaml, link: https://git.example/app }
```

Consumer import (`app/spec.yaml`):

```yaml
system:
  imports:
    - { id: platform, path: ../platform/spec.yaml, link: https://git.example/platform, version: "2.1", capabilities: { run-module: "2.0" } }
```

- Import `path` resolves from the importing spec's directory and shall stay inside the workspace root.
- A standalone clone of `app` has no sibling checkout; `--check-imports` then fails with `(clone <link>)`. Clone first.

## Commands

| Command | Scope |
|---|---|
| `metamodel.py workspace.yaml --workspace [--stage S]` | Every member at `S` (default `release`); cross-member checks below |
| `metamodel.py <member>/spec.yaml --check-imports --root .` | One member's pins against its sibling checkouts |

`--workspace` is offline. It never clones, fetches or executes member code.

Cross-member checks:

- Member IDs unique; paths safe and inside the root; every member loads.
- Every member passes its stage invariants (`Member.stage` when set, else `--stage`) and `--check-imports` (reported as `<member>: ...`).
- Every import targets a member; `link`s agree (trailing `/` and `.git` ignored).
- `system.id` unique across members.
- Import graph between members is acyclic.

Not checked: that links are reachable, artifact/drift checks (run per member with its own `--root`), cross-member `--impact`.

## Hierarchical Specifications (Subsystems)

When a system decomposes into subsystems rather than merely consuming peer APIs, a parent spec models each subsystem as a normal black-box component (`Component.realized_by: <import-id>`):

- **Black-box contract**: in the parent spec, the component declares its `layer`, `domain`, `provides`, `requires` and boundary `owns_data`. `--context <component-id>` outputs only this boundary contract as the child subsystem's discovery input.
- **Export surface**: a child spec may set `system.exports: [<capability-id>, ...]` to restrict which capabilities importing or parent specs can consume; internal capabilities stay private. Omitting `exports` exposes all capabilities.
- **Contract inclusion (`--check-imports`)**:
  - `child.provides ⊇ component.provides`: the child internally provides and exports every capability the parent component provides, with compatible version and contract (`COMPAT-01`..`04`).
  - `child.external_requires ⊆ component.requires`: any external capability required by the child is also declared in the parent component's `requires`.
  - Shared boundary data models (`owns_data` and interface request/response models) satisfy `COMPAT-05`..`07`.
- **Cross-level requirements & maturity**:
  - Parent requirements may reference `<import-id>:<requirement-id>` in `depends_on` or `refines`; when the parent requirement is `verified` and in scope, the referenced child requirement must be verified in the child spec.
  - When a parent component with `realized_by` is `verified` and in `release_scope`, the child spec must pass `release` validation. Deferred components (`release_scope`) and `Member.stage` overrides allow mixed maturity during development.

## Workflow

1. Provider changes a capability: [FEATURE.md](FEATURE.md) in the provider repo, `--compat` against its old spec, bump versions.
2. Provider merges. In the super-repo, update the checkout.
3. Run `--workspace`. Pin errors list each consumer to update.
4. Per consumer: update pins, then re-verify under FEATURE.md (new evidence per [REPLACEMENT.md](REPLACEMENT.md) rules for changed behavior).
5. Run `--workspace` clean before accepting the set.

## Rules

- A requirement lives where its subject is owned. A cross-member behavior is a requirement of the consumer against the imported capability, with evidence in the consumer.
- Mixed maturity is normal: validate members at their own stage with per-member runs; use `--workspace --stage` only for a uniform bar.
- Provider-owned mocks ([COMPONENT-ISOLATION.md](COMPONENT-ISOLATION.md)) are the way a consumer builds without the provider checkout.

Anti-patterns:

- Copying an imported contract (silent fork).
- Pinning a branch or commit instead of version and digest.
- Import cycles between members; extract the shared capability into a third member.
- Putting shared requirements in the workspace repo.
