# Field View Column Toggle Design

## Problem

The trace and log explorer field views no longer provide a reliable way to add a displayed field to the results table. Log promotion is limited to selected attribute keys, trace field promotion is not wired at all, and the existing component-level log test does not protect the browser interaction. Users also cannot remove a promoted column from the field row that added it.

## Scope

- Allow every field displayed in the selected trace or log context panel to be added as a results-table column when the value can be resolved from a record.
- Cover built-in fields and arbitrary attribute or resource-attribute fields.
- Render `+` when the field is absent and `-` when it is present; clicking the action toggles the column.
- Keep the context-panel action, table columns, column picker, and saved log-view configuration synchronized.
- Add regression coverage for trace and log explorers at component and browser levels.

This is a frontend behavior repair. It does not change APIs, persisted telemetry, architecture, deployment, security, or roadmap scope.

## Design

### Shared field action

Extend the shared `DlRow` affordance from a one-way promotion callback to an explicit toggle callback. The button label, icon, title, and accessible name derive from whether the column is currently visible. The action remains a real button and stops click propagation so toggling a column does not close or reselect the current record.

### Column identity and value resolution

Each explorer owns its visible column-key list. Built-in and arbitrary fields use stable keys matching the labels shown in the context panel. A focused resolver for each signal maps a column key to the corresponding record value:

- Logs resolve built-in log fields, `log.*` attributes, and resource attributes.
- Traces resolve built-in root-span fields, span attributes, and resource attributes.

The results tables render the fixed identity columns required for record navigation plus the user-selected field columns. Column additions are de-duplicated and removals preserve the order of all remaining columns.

### State synchronization

The context panel receives the current visible-column list and a toggle callback from its explorer. Toggling updates the same state used by the results table and column picker, so all three surfaces update in one React render. Log saved views continue storing the combined visible-column keys. No new persistence format is introduced for traces in this repair.

## Testing

Follow test-driven development and commit the failing regression tests before the fix.

Component tests for both explorers will:

1. Open a record's context panel.
2. Click `+` on a built-in field and an arbitrary field as applicable.
3. Assert that the corresponding table header and row value appear.
4. Assert that the action changes to `-`.
5. Click `-` and assert that the column disappears.

Browser regression tests will exercise the same user path on the trace and log routes using mocked API data. These tests protect the actual row selection, panel rendering, button interaction, and table update that the existing isolated tests do not cover.

Because component structure and rendered columns change, the existing visual suite will run before and after the implementation and its screenshots will be reviewed. The full `bash scripts/local-ci.sh` gate will run before the code is pushed.

## Repository Impact

- Reuse existing shared and signal components; do not create a parallel field-view component.
- No backend, Testcontainers, generated-model, dependency, ADR, or spec changes are required.
- `docs/agent-context.md` does not need an update because repository layout, ownership, verification rules, and future-agent assumptions remain unchanged.
