# Task 3 Report: Unified Log Field Columns

## Status

Complete. Log table columns now use one ordered `visibleColumns: string[]` state, canonical context keys, and the shared `getLogFieldValue` resolver.

## Changes

- Added `DEFAULT_LOG_COLUMNS` with canonical keys for the existing default table shape: `time`, `severity_number`, `service.name`, and `message`.
- Removed the split built-in/promoted rendering path and `isPromotableLogKey`.
- Made every displayed log context entry toggleable through `DlRow`, including built-ins and arbitrary log/resource attributes.
- Kept the column picker synchronized with built-ins plus selected arbitrary fields.
- Preserved legacy saved-view compatibility by mapping `level` to `severity_number` and `service` to `service.name` when loading.
- Used functional state updates for context-panel toggles.
- Updated focused table and page tests for canonical keys, ordered rendering, built-in removal/addition, and arbitrary-field addition/removal.

## Verification

- RED confirmed before implementation: focused suite failed because `Add service.name as a column` was unavailable.
- `npm test -- --run src/utils/logContext.test.ts src/features/signals/components/LogResultsTable.test.tsx src/pages/LogSearch.test.tsx` — PASS, 3 files / 28 tests.
- `npm run typecheck` — PASS.
- `npm run lint` — PASS.
- `git diff --check` — PASS.

## Self-review

- No trace files were changed.
- No generated files, dependencies, architecture, specs, regression gates, or real dependency boundaries were changed.
- Testcontainers, NLQ eval, cargo checks, and ADR updates are not applicable to this frontend-only focused slice.
- `docs/agent-context.md` does not need an update because repository layout, ownership, verification guidance, and architectural assumptions are unchanged.
- Unrelated `.superpowers/sdd/progress.md` and Python cache changes were left untouched.

## Concerns

- The table now intentionally displays canonical field values and key labels, so `severity_number` is numeric rather than the former derived OTel level label. This follows the task requirement that every selected key render through `getLogFieldValue` and that context labels match headers.
- Full `scripts/local-ci.sh` and visual verification remain for the coordinator's integrated pre-push gate; this task ran the requested focused log suite plus typecheck and lint.

## Important Review Correction Pass

- Preserved canonical `severity_number` identity while restoring the derived, colored OTel label presentation.
- Restored the Message cell's `CopyButton`; both special presentations still source their raw value through `getLogFieldValue`.
- Namespaced resource attributes as `resource.<key>` across context entries, table columns, toggles, and the picker so fixed/log/resource collisions cannot alias.
- Added `normalizeLogColumnKeys` for legacy saved views: maps `level`/`service`, upgrades legacy bare resource keys, and removes mixed canonical/legacy duplicates while preserving first occurrence order.
- Removed the table's independent `showServiceColumn` filtering. `LogExplorer` now derives the initial canonical state once; scoped views initially omit `service.name` but can add it normally from context.
- Added an accessible `No columns selected` header/cell when the ordered selection is empty.
- Added collision, saved-view alias/dedup/order, scoped-view synchronization, severity styling, message-copy, and empty-column tests.

### Correction Verification

- RED confirmed for resource namespace lookup, collision-safe context identities, normalization, restored presentation, and empty-column semantics before implementation.
- `npm test -- --run src/utils/logContext.test.ts src/features/signals/components/LogResultsTable.test.tsx src/pages/LogSearch.test.tsx` — PASS, 3 files / 32 tests.
- `npm run typecheck` — PASS.
- `npm run lint` — PASS.
- `git diff --check` — PASS.
