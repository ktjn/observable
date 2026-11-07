# Business specification baseline v2.5

Immutable snapshot of the vendor-neutral observability business specification adopted by the 2.0 line.

| Field | Value |
|---|---|
| Repository | `ktjn/observability-spec` |
| Version | `2.5` |
| Commit | `523e68212d5600fca8b1a159ac23caddc6e5b735` |
| Snapshot date | 2026-11-07 |
| Files | `spec.yaml`, `spec/**`, `assets/ui/**` exported with `git archive` |
| Integrity | [`MANIFEST.sha256`](MANIFEST.sha256) |

## Rules

- Do not edit files in this directory. A new upstream version is a new sibling directory (`v2.6/`) with its own manifest.
- `spec/system.yaml` does not include this directory. The authoritative 2.0 requirements are the derived fragments in `spec/business/`.
- Every difference between this baseline and the derived fragments is either a declared field policy or a declared refinement in [`../../refinements.yaml`](../../refinements.yaml).
- `scripts/check-business-baseline.py` verifies the manifest and rejects undeclared changes or removals. Additions (new interactions, scenarios or list items) are allowed because they add obligations.
- The baseline validates on its own: `uv run --project models system-spec spec/business/baseline/v2.5/spec.yaml --stage verification`.
- Retiring the upstream repository (Q9) is blocked until this snapshot is merged on `2.0`.
