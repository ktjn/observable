# Experience Specification Example

A user journey across two logical components, written with the optional [experience extension](../../EXPERIENCE.md) ([`spec.md` § 13](../../spec.md#13-experience-specification-optional)).

- **Journey**: `investigate-degraded-service` goes `service-overview` → `service-detail` → `incident-investigation`. It realizes three capabilities provided by two components (`service-catalog`, `incident-store`) and is traced to requirements `req-journey` and `req-journey-denied`.
- **Views** are reusable and state their content, actions and states. States common to all three views (`stale`, `error`) come from the `data-conditions` state set; each view adds its own.
- **Interactions** keep scope (`preserves`) and map each failure mode the user can see to the state that handles it (`unauthorized-evidence` → `service-detail.unauthorized`). A failure that must never occur is an invariant instead (`scope_preserved`). `refresh` is an action inside one view (same source and target). `open-alert` starts at the declared entry `alert-notification`, which is not a view.
- **Scenarios**: positive and negative scenarios exercise every interaction; `critical: true` makes `EXP-05` require that. Each is verified through ordinary requirements; `traverse` names two (`requirements`).
- **References**: `service-overview.svg` is `illustrative` (guidance only); `service-detail.svg` is `normative`, so it carries a version, an acceptance authority (`design-lead`) and a hash.
- **Constraint**: `accessibility: WCAG-2.2-AA` names `req-a11y` as its verifying requirement; the validator checks that requirement's evidence at `release` (`EXP-07`), not the constraint itself.
- **Reference rule**: the overview wireframe stays illustrative but lists `req-overview` in `consistent_with`, so a review can reject it where it contradicts that requirement.

Validate: `metamodel.py examples/experience/spec.yaml --stage verification` and, from the repository root of the example, `--check-artifacts --root examples/experience`. No evidence is supplied, so `--stage release` reports `EXP-07` and the usual missing-evidence errors by design.
