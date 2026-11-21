# Upstream feedback for Designed

Issues and feature requests found while piloting `@ktjn/designed-react` in
observable-web ([designed-adoption.md](designed-adoption.md)). Not filed upstream yet —
this is the running ledger; file issues against https://github.com/ktjn/designed when
ready and link them here.

| # | Component | Issue | Suggested fix | Found in |
| --- | --- | --- | --- | --- |
| 1 | `AlertDialog.Content` (and likely `Dialog.Content`) | Base UI's `initialFocus`/`finalFocus` ref props work at runtime (forwarded via `...rest` to the underlying `Popup`) but aren't declared on `AlertDialogContentProps`. Consumers must cast through an untyped object spread to use them without a type error. | Add `initialFocus`/`finalFocus` to `AlertDialogContentProps` (and `DialogContentProps` if the same gap exists there), typed as Base UI's `DialogPopup` declares them. | `apps/frontend/src/components/ui/confirm-dialog.tsx` |
| 2 | `EmptyState` | No generic `children` slot and no per-slot `className` (`icon`/`title`/`description`/`action` each render fixed markup). A consumer that needs e.g. a metadata-chip list below the description, or a different title size for a "compact" variant, has no way to express it without abandoning the component. | Either accept a `children` slot after `action`, or accept `classNames: { title?, description?, action? }` for per-slot overrides. | Evaluated for `apps/frontend/src/components/ui/empty-state.tsx`; not adopted because of this gap (see [designed-adoption.md](designed-adoption.md) compatibility table) |
| 3 | `@ktjn/designed-react` → `@base-ui/react` | Pinned to an exact version (`"1.9.0"`, no `^`) in `dependencies`, not `peerDependencies`. Exact-pin a transitive dependency makes every designed-react bump a potential duplicate-install/version-skew risk for consumers who also depend on `@base-ui/react` directly (as this repo does, for its own Button/Input work prior to full migration). | Loosen to a caret range, or hoist `@base-ui/react` to a `peerDependency` since designed-react's public API re-exports Base UI types/behavior anyway. | `npm view @ktjn/designed-react@0.6.0 dependencies` during step 1 distribution check |

## Notes
- Only one published version exists for `designed-react`/`designed-tokens`/`designed-icons` (`0.6.0`/`0.6.0`/`0.3.0`) at time of writing — not itself an issue, just means there's no migration-path precedent to learn from yet.
