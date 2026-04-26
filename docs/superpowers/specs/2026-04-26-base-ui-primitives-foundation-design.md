# Base UI Primitives Foundation Design

## Summary

This design defines the first migration slice for the frontend's move to the target design-system stack described in `spec/05-frontend.md` and `spec/15-frontend-local-dev.md`. The slice is intentionally narrow: establish the shared primitive layer and styling foundation without rewriting whole product views.

The implementation must add Base UI and Tailwind CSS v4 to `apps/frontend`, create an owned primitive layer under `apps/frontend/src/components/ui/`, preserve the existing light/dark/system theme contract, and ship only four reusable primitives in this slice: `Button`, `Input`, `Select`, and `Tabs`.

## Problem

The current frontend does not yet match the frontend spec's target architecture:

- interactive controls are mostly native elements styled through global classes in `src/styles.css`
- there is no `src/components/ui/` ownership layer
- Base UI is not installed
- Tailwind CSS v4 is not installed
- primitive behavior and styling are coupled directly to pages and shared layout components

That makes later screen migrations expensive because every route currently re-solves primitive concerns locally.

## Goals

- Add the minimum shared infrastructure needed for future screen migrations.
- Establish the `components/ui` ownership model for primitives.
- Keep the slice small enough to review independently from any feature rewrite.
- Preserve the current `ThemeProvider` contract and current route behavior.
- Prove the foundation with focused tests.

## Non-Goals

- No full-page or feature-by-feature migration to Base UI in this slice.
- No overlay primitives yet: `Dialog`, `Popover`, and `Tooltip` stay out of scope.
- No router, data-fetching, or page-layout redesign.
- No change to the existing theme preference model or theme storage key.
- No attempt to complete the full spec directory reshuffle into `features/*` in the same iteration.

## Recommended Approach

### Option A: Foundation plus four primitives

Install Base UI and Tailwind CSS v4, create the owned primitive layer, and migrate only `Button`, `Input`, `Select`, and `Tabs`.

Why this is the recommended option:

- it proves the ownership model with primitives already used by the existing shell and list/filter views
- it keeps overlay and positioning complexity out of the first slice
- it gives the next migration slice enough building blocks to convert real screens without reopening the foundation

### Option B: Foundation plus two primitives

Install the stack but implement only `Button` and `Input`.

Why not recommended:

- too little behavioral coverage
- `Select` and `Tabs` are both central to the current UI, so the next slice would still need more foundation work before meaningful screen migration

### Option C: Token and dependency setup only

Install Tailwind and Base UI, but do not ship any primitives.

Why not recommended:

- low product value for the slice
- no proof that the chosen foundation actually works in the current app

## Architecture

### 1. Styling foundation

Tailwind CSS v4 becomes the primary styling system for new shared primitives. The existing global stylesheet remains temporarily in place for legacy components and page-level layouts that are not part of this slice.

The migration rule for this slice is:

- new primitives use Tailwind utilities and shared CSS variables
- legacy routes and components may continue using existing classes until later slices migrate them

This avoids a risky "rewrite all styles at once" change.

### 2. Theme contract

The current `ThemeProvider` in `apps/frontend/src/lib/theme.tsx` writes:

- `data-theme="light" | "dark"`
- `data-theme-preference="light" | "dark" | "system"`

That contract remains unchanged.

Tailwind tokens and Base UI-owned primitives must read from CSS custom properties already anchored at `:root` and `:root[data-theme="dark"]`. This keeps:

- persisted theme preference stable
- tests around theme preference valid
- the shell theme toggle behavior unchanged while the primitive layer is introduced

### 3. Owned primitive layer

Add a new directory:

```text
apps/frontend/src/components/ui/
```

This slice will place these components there:

- `button.tsx`
- `input.tsx`
- `select.tsx`
- `tabs.tsx`
- supporting utility module(s) if needed for class composition or shared variants

The primitives are "owned" code in the repo, not thin wrappers around external exports scattered through feature code. Base UI provides accessibility and state primitives; local component files define the Observable-specific API, classes, variants, and composition rules.

### 4. Primitive scope

#### Button

Provide a shared button API for:

- default action buttons
- subtle/secondary action buttons
- destructive action buttons if the current app already needs the variant immediately
- disabled state

The primitive must support being used for the theme selector migration later, but the first slice does not need to migrate that selector yet if doing so expands scope.

#### Input

Provide a text input primitive for current search and filter controls. It must cover:

- common sizing
- placeholder styling
- disabled state
- focus-visible treatment

#### Select

Provide a select primitive for current filter dropdowns. For this first slice, it may stay close to the native `<select>` element if that is the smallest path, as long as it is exposed through the owned `components/ui` API and styled through the new token system.

This slice does not require a fully custom combobox implementation.

#### Tabs

Provide a tabs primitive suitable for service detail and other signal-switching navigation. This is the one place where Base UI behavior matters most in the first slice, because the current app already has tab-like interactions and the spec explicitly calls for Base UI primitives for new interactive components.

The primitive must support:

- list
- trigger
- panel/content composition
- keyboard-accessible selection behavior

## File-Level Design

The first slice should touch only the files needed to establish the foundation cleanly:

- frontend package manifest and lockfile for new dependencies
- Tailwind v4 setup files required by the current toolchain
- `apps/frontend/src/styles.css` or successor style entry for token definitions and Tailwind import
- `apps/frontend/src/components/ui/*` for the new primitives
- minimal shared utility files if needed for class composition
- focused tests for the primitives

It should avoid broad edits across page files. If an app integration touchpoint is needed, keep it to one narrow usage site whose purpose is only to prove the primitive wiring.

## Migration Strategy

This is a foundation slice, not a page migration slice.

The sequence is:

1. install dependencies and Tailwind v4 tooling
2. wire Tailwind into the frontend build
3. preserve and re-home theme tokens so both legacy CSS and new primitives can consume them
4. add `components/ui`
5. implement `Button`, `Input`, `Select`, `Tabs`
6. add tests proving rendering, state, and theme compatibility
7. optionally switch one narrow internal usage site only if needed to validate the primitive in the live app

Later slices can then migrate:

- shell controls
- services filters
- service detail tabs
- explorer toolbars

without revisiting dependency and token setup.

## Testing Strategy

This slice must favor focused component and integration tests over broad UI churn.

Required verification:

- unit/component tests for each primitive
- keyboard/accessibility behavior checks for `Tabs`
- theme compatibility checks proving primitives render correctly under both `data-theme="light"` and `data-theme="dark"`
- frontend `typecheck`, `lint`, `test`, and `build`

MSW is not a major part of this slice because the primitive layer does not introduce new backend contracts.

Accessibility expectations:

- focus-visible states must be obvious
- disabled states must be testable
- tab selection must be keyboard reachable
- labels/roles must align with semantic HTML or Base UI defaults

## Risks And Controls

### Risk: Tailwind introduction breaks existing styling

Control:

- keep legacy global CSS in place for non-migrated surfaces
- use Tailwind only for the new primitive layer in this slice
- avoid mass class rewrites

### Risk: Theme drift between legacy CSS and new primitives

Control:

- keep existing CSS variables as the single token source
- do not redesign theme state or naming
- test both light and dark rendering paths

### Risk: Base UI adds more scope than the slice can absorb

Control:

- limit Base UI usage to primitives where it provides clear value now, especially `Tabs`
- allow `Input` and `Select` to stay implementation-simple behind the owned API

## Acceptance Criteria

This slice is complete when:

- Base UI and Tailwind CSS v4 are installed and wired into `apps/frontend`
- the app still supports the current light/dark/system preference flow
- `apps/frontend/src/components/ui/` exists and contains owned `Button`, `Input`, `Select`, and `Tabs` primitives
- the primitives are covered by focused tests
- frontend verification passes locally
- no overlay primitives or broad screen migrations are bundled into the same PR

## Follow-On Slices

The next smallest useful slice after this one should migrate a limited real usage surface onto the new primitives, likely one of:

- theme and shell controls
- services catalog filters
- service detail signal tabs

That follow-on slice should consume the new primitives rather than expand the foundation again.
