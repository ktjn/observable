# Phase 4 (Cleanup & Documentation) — Modelable Migration Design

> Companion to `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`, Phase 4.

## Context

Phase 3 of the modelable type-mapping migration is complete: 9 of 10 in-scope domains
(tracing was Phase 2's pilot; logs, metrics, notifications, admin/members, slos, incidents,
alerts, dashboards, nlq/visualization were Phase 3) have `.mdl` sources under `models/` and
generated TypeScript artifacts under `apps/frontend/src/api/generated/<domain>/`. Two domains
(tracing, logs) also have generated Rust `db`-projection Row types under
`libs/domain/src/generated/<domain>/`. `3.5b Schemas` remains deliberately deferred (no
frontend consumer exists for `SchemaEntry`/`SemanticAnnotation` yet).

Phase 4 has four checklist items in the migration plan (lines 174-179):

1. Remove dead hand-written struct/interface definitions and `From`/`Into` impls once each
   domain's generated code is verified in place.
2. Update `docs/agent-context.md` with the generated-types convention and where `.mdl` sources
   live.
3. Add an ADR documenting modelable's adoption as the type-mapping source of truth.
4. Cross-reference `spec/adr/ADR-030-timestamp-representation.md` if generated types change
   how that convention is expressed/enforced.

**Item 1 audit (done during brainstorming, this session):** a read-only investigation across
all 9 Phase-3 domains plus tracing/logs found **no removable dead code**. Every domain's
hand-written type was either turned into a re-export/type-alias *as part of its own task* (TS
layer, all 10 domains) or kept hand-written with a lineage doc comment per the plan's
documented per-domain rationale (Rust layer, 8/10 domains — blocked on Phase 1 backlog items
3/5/8/9). The two `#![allow(dead_code)]` blocks in `libs/domain/src/generated/{tracing,logs}.rs`
are intentional per the Phase 2.3 "clean-diff regeneration" decision. `From`/`Into` impls in
`span.rs`/`log.rs` are still actively called. **Item 1 requires no code changes** — only a note
in the migration plan recording that the audit was performed and found clean.

Items 2-4 are net-new/updated documentation. This design covers all four.

## Deliverables

### 1. `spec/adr/ADR-032-modelable-type-mapping-adoption.md` (new)

Standard ADR following the existing format (see `ADR-030`/`ADR-031` for structure/tone:
Date/Status/Authors/Deciders/Review date header, then Context/Decision/Consequences/
Alternatives Considered/Related).

- **Date:** 2026-06-15. **Status:** Accepted. **Authors:** ktjn. **Deciders:** Project
  Stakeholders. **Review date:** 2027-06-15.

- **Context:** Restate the original audit finding that motivated the migration plan: 19 domain
  types, 47 backend API types, 41 frontend interfaces, only ~6 explicit `From`/`Into` mappings
  between layers — drift risk between Rust domain structs, DB row structs, API
  request/response structs, and hand-written frontend TS interfaces. Introduce
  [modelable](https://github.com/ktjn/modelable) as a declarative compiler: `.mdl` models in
  `models/` compiled to per-language artifacts with field-level lineage and breaking-change
  detection.

- **Decision:** Adopt modelable `.mdl` files as the canonical source of truth for shared
  domain/wire types, migrated domain-by-domain (Phase 2 pilot + Phase 3 full migration, per
  `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`). Specifically:
  - Generated Rust/TypeScript artifacts are **committed to the repo** (not built in CI),
    verified clean via `modelable compile` diff-check.
  - **Per-domain rule:** only replace types representing canonical domain/wire contracts.
    Handler-level aggregation/wrapper types with no 1:1 generated equivalent (e.g.
    `TraceResponse`, `LogListResponse`, `IncidentListResponse`) stay hand-written.
  - `@wire(...)` hints (e.g. `json.fieldCase: "snake_case"`) bridge representation gaps for a
    single target without affecting others.
  - Python/uv + a modelable checkout is a **dev-time-only** dependency for editing `.mdl`
    sources or regenerating artifacts — not a CI/build dependency for unrelated changes.

- **Current State** (table, one row per domain):

  | Domain | `.mdl` | Generated TS | Generated Rust | Notes |
  |---|---|---|---|---|
  | tracing (Span/SpanEvent) | `models/tracing.mdl` | yes (`generated/tracing/`) | yes — `db`-projection Row types only (`generated/tracing.rs`) | `Span`/`SpanEvent`/enums + From/Into hand-written (Phase 2.3 scope note) |
  | logs (LogRecord) | `models/logs.mdl` | yes | yes — `LogRow` alias (`generated/logs.rs`) | `LogRecord` hand-written, From/Into to Row hand-written |
  | metrics (MetricPoint) | `models/metrics.mdl` | yes | no | `MetricSeries`/`MetricPointRow`/etc. hand-written — blocked on backlog #1-3 |
  | notifications (NotificationChannel) | `models/notifications.mdl` | yes | no | handler types + `From` impl hand-written — backlog #3 |
  | admin/members (Member) | `models/admin.mdl` | yes | no | `joined_at` timestamp — backlog #5 |
  | slos (SloDefinition) | `models/slos.mdl` | yes | no | timestamp fields — backlog #5 |
  | schemas | — | — | — | **3.5b deferred** — no frontend consumer for `SchemaEntry`/`SemanticAnnotation` |
  | incidents (Incident/IncidentEvent) | `models/incidents.mdl` | yes | no | timestamp fields — backlog #5 |
  | alerts (AlertRule/Firing) | `models/alerts.mdl` | yes | no | timestamp fields — backlog #5 |
  | dashboards (Dashboard/DashboardPanel/...) | `models/dashboards.mdl` | yes | no | timestamp fields — backlog #5; cross-model imports — backlog #7 |
  | nlq/visualization (NlqIr/NlqFilter/NlqTimeRange/FieldRole) | `models/nlq.mdl` | yes | no | `Option<T>` nulls — backlog #8; `array<enum(...))` — backlog #9 |

- **Known Limitations** (condensed from Phase 1 backlog, each linking to
  `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md`'s "Phase 1
  backlog" section by item number for full detail):
  1. Array-element `rust.type` hints not supported (blocks `Vec<u64>` histogram fields).
  2. No non-optional default-empty-array projection fields.
  3. `enum(...)` always emits Rust `String`, never a real enum.
  4. Duplicate `binding` declarations break from-scratch registry builds.
  5. `timestamp` emits Rust `String`, not `chrono`-compatible.
  6. `enum(...)` members can't start with a digit (blocks e.g. `"5m"` preset unions).
  7. TS emitter doesn't emit imports for cross-model `NamedType` references (manual
     workaround in place for `dashboards.Dashboard.v1.ts`).
  8. `Option<T>` can't express "always-present, possibly-null" (`T | null`) — only omittable
     (`T | undefined`).
  9. `array<enum(...))` emits invalid TypeScript operator precedence.

  These gaps are why 8/10 domains' Rust layers remain hand-written (with lineage doc comments
  only) and why `NlqIr`/`FieldRole` etc. needed hand-written TS extensions.

- **Consequences:**
  - *Easier:* TypeScript types for 10 domains can no longer silently drift from their Rust
    counterparts — `modelable lineage` proves every generated field traces to its `.mdl`
    source. Adding a field to a migrated domain means editing one `.mdl` file and regenerating,
    not hand-editing N call sites.
  - *Harder:* Rust-side migration is incomplete for 8/10 domains pending modelable emitter work
    (Phase 1 backlog). Anyone editing `.mdl` files needs a modelable checkout + Python/uv
    locally (dev-time only, not CI-gating per "Resolved Decisions" in the migration plan).

- **Alternatives Considered:**
  - *Status quo (hand-written, manually audited):* rejected — this is the drift problem that
    motivated the migration; manual audits don't scale and don't prevent regressions.
  - *OpenAPI/protobuf-based codegen:* rejected — neither models the `db` ↔ canonical ↔
    `reply`/`event` auto-projection distinction Observable's layered architecture needs
    (ClickHouse/Postgres row shapes differ from API response shapes differ from domain
    structs); adopting either would require building that projection concept from scratch,
    which is exactly modelable's value-add.

- **Related:**
  - `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — full
    migration plan, Phase 1 backlog, per-domain design specs.
  - `ADR-030: Timestamp Representation` — the Unix-nanoseconds-as-string convention is
    **unaffected** by this migration; no `.mdl` model currently overrides it (see Phase 1
    backlog item 5, which is about a *different* `timestamp` IDL type used for
    `created_at`/`joined_at`-style fields, not the ADR-030 telemetry-timestamp convention).
  - Each domain's design spec under `docs/superpowers/specs/2026-06-1{2,3,4,5}-*-modelable-
    migration-design.md`.

### 2. `spec/adr/ADR-030-timestamp-representation.md` — add cross-reference

In the "Related" section (after the existing `apps/frontend/src/utils/formatTimestamp.ts` /
`apps/frontend/src/lib/timeDisplay.tsx` lines), add:

```markdown
- `ADR-032: Adopt Modelable as Type-Mapping Source of Truth` — the nanosecond-as-string
  convention described here is preserved unchanged by the modelable migration; no `.mdl`
  model overrides it.
```

### 3. `docs/agent-context.md` — rewrite "Modelable Type-Mapping Migration" section

Replace the section currently at lines 151-191 (header through the Phase 3 domain list). New
content:

- **Header:** `## Modelable Type-Mapping Migration (Phase 3 complete, 2026-06-15)`
- Opening paragraph: update from "tracing domain is the worked template... Phase 3 upcoming"
  to: all 10 in-scope domains migrated (TS layer; tracing+logs also Rust `db`-row layer), 3.5b
  Schemas deferred (no frontend consumer). Point to `ADR-032` for the full decision record and
  current-state table, and to the migration plan for the Phase 1 backlog / per-domain specs.
- Keep the existing bullet points on **Model sources**, **Generated Rust artifacts**,
  **Generated TypeScript artifacts**, **`@wire(...)` hints**, **Per-domain rule**, and
  **Verification** — these conventions remain accurate and are still the reference for any
  future domain work (e.g. if 3.5b is picked up, or Rust generation is extended per the
  backlog).
- Replace the final bullet (currently "Phase 3 ... lists the remaining domains in migration
  order ... each follows this same template via its own brainstorm → spec → plan →
  subagent-driven-implementation cycle") with: a completed-domains list (tracing, logs,
  metrics, notifications, admin/members, slos, incidents, alerts, dashboards,
  nlq/visualization) + the 3.5b-deferred note + a pointer to `ADR-032`'s Known Limitations
  section for what blocks further Rust-layer migration.

### 4. `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — check off Phase 4

Change all four Phase 4 checkboxes (lines 176-179) from `- [ ]` to `- [x]`, each with a short
parenthetical:

- Item 1: note that a dead-code audit (this session) found nothing removable — cite the two
  intentional `#![allow(dead_code)]` blocks and the still-active `From`/`Into` impls in
  `span.rs`/`log.rs` as the only candidates considered, both correctly kept per Phase 2.3/3.1.
- Item 2: note `docs/agent-context.md`'s "Modelable Type-Mapping Migration" section was
  rewritten to reflect Phase 3 completion and points to `ADR-032`.
- Item 3: note `ADR-032` was added.
- Item 4: note `ADR-030`'s "Related" section now cross-references `ADR-032`; convention
  unchanged.

## Testing / Verification

This is a documentation-only change — no code, no `.mdl` changes, no generated-artifact
regeneration. Verification is:
- Markdown files render correctly (visual read-through).
- `bash scripts/local-ci.sh` still passes (no behavioral change expected; included for the
  AGENTS.md "mandatory before every push" rule, since this still results in a commit/PR).
- Cross-references resolve: `ADR-032` ↔ `ADR-030` links are bidirectional and correct;
  `docs/agent-context.md` → `ADR-032` link is correct.

## Out of Scope

- Any further Rust-layer migration for the 8 domains still blocked on Phase 1 backlog items
  (separate future work, not part of Phase 4).
- 3.5b Schemas (deliberately deferred, unchanged by this work).
- Any changes to `.mdl` files or generated artifacts.
