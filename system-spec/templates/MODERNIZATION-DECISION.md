# Mechanism decision record

Copy one per material mechanism beside the specification ([MODERNIZATION.md](../MODERNIZATION.md)). Review records, not metamodel YAML fields.

| Record | Value |
|---|---|
| ID | `MECH-<nn>`; referenced by `Refinement.decision` or a gate decision record |
| Mechanism | Algorithm, data structure, storage, protocol, framework or topology |
| Components / source | Component IDs, source paths and revision |
| Forces | Requirement, invariant, risk and quality-attribute IDs it serves; `none` = accidental |
| As-is evidence | Measurements and their evidence IDs, conditions and limits |
| Original assumptions | Load, data volume, platform, team, cost; which still hold |
| Alternatives | Option, trade-off against each force, evaluation evidence (spike revision, benchmark, model-check) |
| Decision | `keep`, `replace`, `investigate` or `retire`; reason |
| Route | FEATURE.md change (obligation changes) or REPLACEMENT.md slice (implementation only) |
| Owner / follow-up | Decision owner, next action, expiry for `investigate` |
| Findings | Hidden coupling or model gaps ([feedback](../CONSTITUTION.md#self-improvement-and-model-feedback)) |
