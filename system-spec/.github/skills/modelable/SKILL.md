---
name: modelable
description: Derive Modelable `.mdl` from a component's metamodel data_models to generate typed artifacts and check field-level version compatibility, recording results as metamodel Evidence. Use at implementation (greenfield phase 9) and before publishing a new data model version (incl. brownfield Level 2); not for specification, where data_models are authoritative. Where `.mdl` files already exist and drive code generation (brownfield), compare them with `data_models` instead of regenerating them (§ Direction).
---

# Modelable Data Contracts

Rules: [constitution § Data Contracts](../../../CONSTITUTION.md#data-contracts). Format: [`spec.md`](../../../spec.md) §§ 5, 8. Tool: [Modelable](https://github.com/ktjn/modelable).

## Direction

- **Greenfield**: `data_models` in the metamodel YAML are the source of truth; `.mdl` is a derived implementation artifact kept with the component's code. Follow § Steps.
- **Brownfield, `.mdl` authoritative**: hand-maintained `.mdl` files predate the spec and the build generates code from them; `data_models` were read from them. Do **not** regenerate them (Steps 1 and 6 would overwrite the authority with a copy). Follow § Brownfield.
- Say which direction applies per data model (a note on the model or in the intake); a mixed system can have both.

## Inputs

- Target `data_models` entries (validated with `metamodel.py`): `id`, `version`, `owner`, `fields`, `classification`, `derived_from`.
- Requirement(s) to verify, with `criteria`.
- Brownfield: existing schemas or DDL (`modelable import`) to compare against `data_models`.

## Mapping

| Metamodel | `.mdl` |
|---|---|
| `owner` (component ID) | `domain` with `owner: "<component>"` |
| `id`, `version` `"N.0"` | `entity`/`event`/`value` `<Name> @ N` |
| `fields` | Typed fields; exactly one `@key` for entities |
| `classification` | `@classification("…")`/`@pii` on fields; model level = strictest field. `open` → `public`; `secret` → `restricted` |
| `derived_from` | `projection … from <src> @ N` / `join <src> @ N`; fields use `<-` or `=` |

## Steps

1. **Derive**: one `domain` per owning component; exactly the fields of `data_models`, never more.
   - Annotate `@pii`/`@classification` explicitly on every projection field; it is not inherited by generated artifacts.
2. **Validate**: `modelable validate <dir> --strict`.
3. **Lineage**: `modelable lineage <domain>.<Projection>@N --path <dir>`; sources must match `derived_from`, otherwise fix the metamodel first.
4. **Compatibility** (new version): `modelable diff <ref>@N <ref>@N+1 --path <dir>` and `modelable validate-compat --from <old> --to <new> --target <json-schema|protobuf|openapi|…>`. Exit `1` = breaking; bump the `data_models` major version and run `metamodel.py --impact <id>`.
5. **Generate**: `modelable compile <dir> --target <target> --out <dir>`; keep `modelable-artifact-manifest.json`.
6. **Drift** (greenfield direction): any change starts in `data_models`; regenerate `.mdl` and artifacts, never edit them first.

## Brownfield

When `.mdl` (or other schemas) already exist and generate code:

1. **Compare, don't regenerate**: check each `.mdl` entity against its `data_models` entry: id and `@ N` ↔ `version`, fields and types, `@key`, `domain owner` ↔ `owner`, `@pii`/`@classification` ↔ `classification` (strictest field), projections ↔ `derived_from`. Run `modelable validate <dir> --strict` and `modelable lineage` as in Steps 2–3.
2. **Resolve each difference as drift**: the `.mdl` is the authority, so correct `data_models` (and run `metamodel.py --impact <data-model>`); change the `.mdl` only through the project's own change process, then update `data_models` to match. A difference you cannot explain is a finding, not something to overwrite.
3. **Compatibility**: `modelable diff` and `validate-compat` between the deployed and the proposed `.mdl` version (as in Step 4) give `analysis` evidence for serialization and compatibility requirements; keep the tool output with it.
4. **Hand over** to `data_models` (Greenfield direction) only as a deliberate decision: the owning team agrees, the `.mdl` files are regenerated once from `data_models` with no field change (`diff` is empty, generated artifacts byte-equal or accepted), and the build switches to the regenerated files. Record the decision; until then the `.mdl` stays authoritative.

## Evidence

```yaml
evidence:
  - id: ev-order-compat
    requirement_id: req-order-contract
    requirement_version: "2.0"
    method: analysis
    passed: true
    provenance: modelable::validate-compat --from v1 --to v2 --target json-schema
```

- `diff`/`validate-compat` → `analysis`; generated serializer tests → `test`; a clean brownfield comparison (step 1, no unresolved difference) → `inspection` of the `.mdl` against `data_models`.
- `provenance` identifies the command and sources; keep tool version and output with it ([`spec.md` § 8](../../../spec.md#8-verification)).
- Model changes invalidate the evidence; rerun after `--impact <data-model>`.

## Notes

- Requires Python 3.14: `uvx modelable`. If import fails on `selectolax`, use `uvx --with 'selectolax<1' modelable`.
- `(additive)` versions reject removals; declare breaking versions explicitly.

## Prompt

"Derive `.mdl` for `<data-model>` from its `data_models` entry; one domain per owner, no invented fields, explicit `@pii`/`@classification` on every field and projection field; then compile to `<target>`."
