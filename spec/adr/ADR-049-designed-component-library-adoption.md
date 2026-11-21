# ADR-049: Incremental Adoption of the Designed Component Library

**Date:** 2026-11-21
**Status:** Accepted
**Authors:** Observable platform, Claude Sonnet 5
**Deciders:** Observable project stakeholders
**Review date:** 2027-02-21

## Context

`apps/frontend` hand-rolls every UI primitive (`components/ui/*`) and its own theme palette (three
hand-maintained `:root[data-theme]` CSS custom-property blocks in `styles.css`, including a VT220
retro skin). [ADR-006](ADR-006-react-vite-frontend.md) chose Tailwind CSS v4 "over CSS Modules" for
styling everything the frontend itself authors.

[`docs/plans/designed-adoption.md`](../../docs/plans/designed-adoption.md) is the working evaluation
record: a pilot adopting `@ktjn/designed-react`/`@ktjn/designed-tokens` (a separate, externally
versioned React 19 + Base UI component library published to GitHub Packages) incrementally, control
by control, instead of a wholesale rewrite. This ADR records the outcome once enough controls were
piloted to judge the pattern, not a decision made in advance of evidence.

GitHub Packages' npm registry requires an authenticated token for every read, even of public
packages (unlike `ghcr.io`'s anonymous pulls) — resolved using the existing Actions `GITHUB_TOKEN`
(same-owner `read:packages`), no new secret.

## Decision

1. **Adopt `@ktjn/designed-react`/`@ktjn/designed-tokens` as a dependency**, consumed incrementally
   per control family, each evaluated on its own contract-preservation merits — not a blanket
   replacement of `components/ui/*`.
2. **Theme palette**: retire Observable's hand-maintained `:root[data-theme]` custom properties and
   the VT220 skin entirely. Designed's `data-color-scheme` tokens are the sole source of truth for
   color. Observable's existing `--bg`/`--text`/etc. custom-property *names* are kept as a transitional
   alias layer onto `--ds-color-*` tokens in `styles.css`, so the ~950 lines of existing component CSS
   that reference them keep working; the alias — not the hand-rolled hex values — is what gets deleted
   as each consumer migrates to the `ds-*` tokens directly.
3. **Swap when the contract survives intact**: Button and Input moved to `@ktjn/designed-react` behind
   their existing module paths and public API (ref, default type, disabled/form behavior, consumer
   class overrides, `destructive`→`danger` variant rename only). ConfirmDialog moved to Designed's
   `AlertDialog` (Base UI underneath): real focus trap/portal/`initialFocus` replaced a hand-rolled
   `querySelectorAll`-based tab trap, preserving the Cancel-first-focus accidental-confirmation guard
   (`OBS-24-03`) and fixing a pre-existing e2e bug as a side effect.
4. **Don't swap when the contract can't survive**: Badge, HealthDot, EmptyState, and the command
   palette keep their Observable implementations. Each was evaluated and rejected for a concrete,
   documented reason (no generic children slot, no per-slot `className`, or — for the command
   palette — no way to intercept query state for live async search with heterogeneous per-type
   rendering). Record mismatches in the plan doc; don't force a swap to satisfy a general preference
   for the dependency.
5. **Amends [ADR-006](ADR-006-react-vite-frontend.md)'s styling choice**: Tailwind remains Observable's
   own authored styling approach for everything it writes itself. Designed's CSS Modules are a producer
   implementation detail Observable doesn't author or maintain — the same way a compiled npm dependency's
   internal build tooling isn't "Observable's" build tooling. This is not a reversal of ADR-006; Observable
   writes zero new CSS Modules.
6. **Known cost, not hidden**: overlay components pull in Base UI's floating-focus-manager, a real
   per-adoption bundle cost (~15.7 kB gzip for the ConfirmDialog swap) distinct from Button/Input's
   near-zero cost. Evaluate each control family's bundle delta, don't assume uniform cost across families.

## Consequences

**Easier:**
- Button, Input, and overlay focus/portal/trap behavior no longer hand-maintained; Base UI's
  accessibility primitives (verified via the existing Playwright a11y suite) replace bespoke
  `querySelectorAll` focus-trap code that was one edge case away from a regression.
- One theme-token source of truth instead of three hand-maintained palettes; a WCAG contrast bug
  (`--brand` aliased to an action-button blue, failing 4.5:1 as link/badge text) surfaced and got
  fixed during the pilot specifically *because* the token source became centralized and auditable.

**Harder:**
- Two component-provenance patterns now coexist permanently in `components/ui/*`: Observable-authored
  (Tailwind) and Designed-wrapped (CSS Modules + Tailwind for layout only). Contributors must check
  which a given component is before assuming Tailwind utility overrides will out-cascade it (layer
  order handles this correctly today — `ds` precedes `components`/`utilities` — but it's a new rule
  to know).
- `@ktjn/*` packages require `read:packages`-scoped GitHub auth for every install, including local
  dev (`GH_PKG_TOKEN`) and CI — one more credential-shaped thing that can silently break a contributor's
  first `npm install`.
- VT220 theme is gone. No migration path back short of reverting this ADR's theme-palette decision.

**Constrained:**
- Future control adoption must repeat this ADR's evaluate-first discipline (write/confirm the existing
  test suite as the contract, measure bundle delta, check for a real children/slot mismatch) rather
  than treating "Designed has a component with this name" as sufficient reason to swap.
- Designed publishes a single version today (`0.6.0`/`0.6.0`/`0.3.0` for react/tokens/icons) with no
  migration precedent; upgrading exact pinned versions requires the same re-verification this pilot did,
  not a routine dependency bump.

## Alternatives Considered

### Option A: Wholesale rewrite of `components/ui/*` onto Designed
Rejected: Badge/EmptyState/command-palette's rejected-swap findings show Designed's API surface doesn't
cover Observable's actual requirements (dense-table severity indicators, async heterogeneous search).
A forced wholesale swap would have meant rebuilding those requirements as custom Designed wrappers —
more code than keeping Observable's existing implementation, for no behavioral gain.

### Option B: Don't adopt Designed at all; keep hand-rolling everything
Rejected: the ConfirmDialog swap fixed a real pre-existing accessibility/focus-trap risk class
(hand-rolled `querySelectorAll` Tab-trapping) that Base UI's maintained primitives close out for free
on every future overlay, not just this one.

### Option C: Fork/vendor Designed's components instead of depending on the published package
Rejected: loses upstream fixes and widens the maintenance surface this ADR is explicitly trying to
shrink; the GitHub Packages auth friction (Consequences, above) is a one-time integration cost, not
a recurring one.

## Related

- [`docs/plans/designed-adoption.md`](../../docs/plans/designed-adoption.md) — the live compatibility-findings table and implementation sequence this ADR summarizes
- [`docs/plans/designed-upstream-feedback.md`](../../docs/plans/designed-upstream-feedback.md) — gaps found in Designed's API worth filing upstream (untyped `initialFocus`/`finalFocus`, `EmptyState`'s missing slots, `CommandPalette`'s missing query hook)
- [ADR-006](ADR-006-react-vite-frontend.md) — amended styling-choice context (§ Decision point 5, above)
- [ADR-036](ADR-036-frontend-module-boundaries.md) — module-boundary constraints this adoption stayed inside (no changes to routing/query-state ownership)
