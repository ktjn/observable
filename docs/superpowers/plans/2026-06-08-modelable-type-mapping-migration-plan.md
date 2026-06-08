# Adopt Modelable for All Type Mapping — Migration Plan

> **Status:** Proposed. Phase 1 (modelable extension) tracked upstream at github.com/ktjn/modelable; this document is the Observable-side companion plan.
>
> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Each numbered step is its own small commit/PR per `AGENTS.md` "Branch and PR Every Iteration". Do not start a Phase-2/3 step until its Phase-1 prerequisite has shipped in a tagged modelable release.

**Goal:** Replace Observable's hand-written, drift-prone type-mapping layers (DB row structs, domain structs, API request/response structs, duplicated TypeScript interfaces) with a single declarative source of truth — `.mdl` models compiled by [modelable](https://github.com/ktjn/modelable) — covering Rust and TypeScript, migrated domain-by-domain in small, independently-verifiable steps.

**Why now:** An audit found 19 domain types, 47 backend API types, and 41 frontend interfaces with only ~6 explicit `From`/`Into` mappings between them — the rest is inline, untracked, and prone to silent drift between layers (confirmed gaps documented in the Context section below).

---

## Context

Observable hand-writes every type-mapping layer:
- ClickHouse/Postgres row structs (`sqlx::FromRow` / `clickhouse::Row`) in `services/query-api/src/*.rs` and `libs/domain/src/*.rs`
- Domain structs in `libs/domain/src/{span,log,metric,nlq,envelope,visualization,telemetry}.rs`
- API request/response structs per handler module in `services/query-api/src/*.rs`
- Duplicate TypeScript interfaces in `apps/frontend/src/api/*.ts`

`modelable` is a declarative compiler that defines canonical domain models in `.mdl` and generates per-language artifacts with field-level lineage and breaking-change detection — exactly the single-source-of-truth role this codebase is missing.

**Gap analysis — modelable's Rust emitter (`cli/src/modelable/emitters/rust.py`) currently only emits bare `#[derive(Debug, Clone, PartialEq)] pub struct` shapes.** It does not yet generate:
- `serde::Serialize`/`Deserialize` derives, `rename_all`, or per-field `#[serde(with = ...)]` (needed for [ADR-030](../../../spec/adr/ADR-030-timestamp-representation.md)'s u64-nanosecond-as-string timestamp convention and ClickHouse UUID encoding)
- `sqlx::FromRow` / `clickhouse::Row` derives
- `From`/`Into`/`TryFrom` impls between auto-projections (`db` ↔ canonical ↔ `request`/`reply`/`event`) — the actual conversion code
- SQL DDL for the `db` auto-projection

modelable's own design-spec work for closing these gaps is tracked at github.com/ktjn/modelable PR #37 (`docs/superpowers/specs/2026-06-08-target-serialization-hints-design.md`), which documents — with concrete examples pulled from this codebase (`libs/domain/src/span.rs`, `metric.rs`) — that the mapping is **not** a 1:1 shape translation: the same field (e.g. a u64 timestamp, an enum, a UUID, a `HashMap<String, Value>`) has up to three different wire representations across domain/storage/transport layers, some mechanical (closed-vocabulary `@wire` hints) and some irregular (CEL projections / a `@manual` escape hatch).

**Sequencing decision:** extend modelable first and cut a release, then migrate Observable domain-by-domain — covering Rust and TypeScript together per domain, with the full type inventory enumerated up front (below) rather than discovered incrementally. Steps 1.2–1.8 land as separate PRs upstream but ship to Observable as **one batched release** (1.9) — not one pin-bump per emitter feature — so Phase 2 starts from a single, fully-capable baseline instead of chasing a moving dependency.

## Open Decisions to Resolve Before Phase 2 Starts

1. **Generated-artifact commit strategy.** The modelable repo's `docs/consuming-modelable.md` advises against committing generated output "unless you have an existing convention." Plain `cargo build` has no codegen hook, and the frontend build has no `.mdl`-aware step either — so either (a) generated `.rs`/`.ts` files are committed alongside their `.mdl` sources and regeneration is a manual/CI-checked step (simplest, matches how this repo already treats most generated artifacts), or (b) a `build.rs` / pre-build script runs `modelable compile` on the fly (keeps generated code out of diffs, but adds a Python/uv runtime dependency to every `cargo build` and `npm run build`). Decide and record the choice — and the reasoning — in step 2.1, since it shapes every later step's PR diff shape.
2. **Python/uv becomes a CI-gating toolchain dependency.** Observable already uses Python for ad-hoc tooling (`scripts/nlq-eval.py`, seed scripts under `scripts/seed/`), but `scripts/local-ci.sh` does not currently invoke Python — it's a Rust+TS build. Wiring `modelable validate`/`compile` into `local-ci.sh` (step 2.1) makes Python+uv+modelable a hard prerequisite for local CI to pass. This is a real architectural addition, not just a tooling tweak — name it explicitly in the Phase 4 ADR (and in `docs/agent-context.md`) so future agents don't treat it as incidental.

---

## Phase 1 — Extend modelable (tracked in github.com/ktjn/modelable, not this repo)

Each step below is its own small PR in the modelable repo, gated by that repo's `AGENTS.md`/`agent-governance.md` (emitter test conventions in `cli/tests/`, Docker-backed compile smoke tests for codegen changes).

> **Scope reality check (confirmed by reading `docs/emitter-spec.md` §2 directly):** the *approved* Phase 1 emitter scope is **JSON Schema, TypeScript-via-JSON-Schema, and Markdown only**. Rust does not appear in that phase table at all (it's mentioned separately in §9 as one of several targets "implemented in the local codegen boundary"), and **SQL DDL is explicitly listed as Phase 5, Deferred**. Steps 1.3–1.6 and 1.8 below therefore are *not* drop-in Phase 1 work — they require modelable's maintainers to either formally extend approved scope to cover Rust (and, for 1.8, pull SQL DDL forward from Phase 5) or accept this as an explicitly-sponsored out-of-band track. Step 1.2a makes that decision a named, visible gate rather than an assumption — see also the scope note added to PR #37 (`docs/superpowers/specs/2026-06-08-target-serialization-hints-design.md` → Dependencies/Sequencing).

- [x] **1.1 Design spec for serialization hints** — github.com/ktjn/modelable PR #37. Proposes closed-vocabulary `@wire(target: encoding, ...)` annotations, an `overrides` table for irregular enum strings, and CEL/`@manual` for whole-type custom conversions; documents which parts are in approved scope vs. need a scope decision (see above).
- [ ] **1.2 IDL + IR support** — extend `cli/src/modelable/parser/ir.py` (`FieldDef`) and the validator to parse, validate, and carry the new `@wire`/`@manual` hints. *(In approved scope — JSON Schema/TypeScript need these hints too, not just Rust.)*
- [ ] **1.2a Resolve the Rust/SQL-DDL scope question** — get an explicit, documented decision from modelable's maintainers (an `emitter-spec.md` table update, or an equivalent written sign-off) on whether extending the Rust emitter and pulling SQL DDL forward from Phase 5 are accepted as in-scope for this effort. **Do not start 1.3–1.6 or 1.8 until this lands** — they are the steps this plan's "extend modelable first" sequencing most depends on, and starting them speculatively risks unreviewable upstream churn.
- [ ] **1.3 JSON Schema emitter: `@wire(json: "string")` support** — emit `{"type": "string", "x-modelable-wire": {...}}` instead of `{"type": "integer"}` for hinted `int` fields, so `json-schema-to-typescript` naturally produces `string` for ADR-030's nanosecond-timestamp-as-string convention. *(In approved scope — this is the mechanism that fixes the TypeScript path; per `docs/emitter-spec.md` §7 "do not hand-roll a separate TypeScript type mapper," there is no independent TypeScript-emitter step needed for `json` hints.)*
- [ ] **1.4 *(blocked on 1.2a)* Rust emitter: serde support** — emit `#[derive(Serialize, Deserialize)]`, `#[serde(rename_all = ...)]`, per-field `#[serde(with = "...")]`/`#[serde(rename = "...")]` driven by the hints, incl. the new `rust.type` hint for signedness overrides (`u64` vs. the IDL's native 64-bit `int`) (`cli/src/modelable/emitters/rust.py`).
- [ ] **1.5 *(blocked on 1.2a)* Rust emitter: `sqlx::FromRow` support** — generation mode for Postgres-bound projections, building on the existing binding/adapter concept (`docs/migration-guide.md` §5, `cli/src/modelable/runtime/adapter/postgres.py`).
- [ ] **1.6 *(blocked on 1.2a)* Rust emitter: `clickhouse::Row` support** — generation mode for ClickHouse-bound projections, incl. `#[serde(with = "clickhouse::serde::uuid")]`-style attributes.
- [ ] **1.7 *(blocked on 1.2a)* Rust emitter: generated `From`/`Into` impls** — conversion code between a model's auto-projections (`db` ↔ canonical ↔ `request`/`reply`/`event`), mirroring `libs/domain/src/span.rs:58-137`. This is the actual type-mapping deliverable — the rest is scaffolding for it.
- [ ] **1.8 *(blocked on 1.2a; conflicts with the documented Phase 5 deferral unless 1.2a explicitly overrides it)* SQL DDL emitter** — `CREATE TABLE` generation for the `db` auto-projection (Postgres + ClickHouse), so migrations can be generated rather than hand-written.
- [ ] **1.9 Cut and tag a modelable release** per the **modelable repo's** `docs/consuming-modelable.md` (github.com/ktjn/modelable/blob/main/docs/consuming-modelable.md — confirmed present at that path), so this repo can pin to it (`AGENTS.md` requires "use the latest stable versions… install from GitHub releases").

## Phase 2 — Pilot migration: Spans/Traces (this repo, template for all later domains)

**Branch:** `feat/modelable-pilot-spans`. Chosen because it's a complete vertical slice: ClickHouse row (`SpanRow`) → domain struct (`Span`) → API response (`TraceResponse`) → hand-written TS interface.

- [ ] **2.1** Pin modelable as a dev dependency per the modelable repo's `docs/consuming-modelable.md`; wire `modelable validate`/`compile` into `scripts/local-ci.sh` (gated, e.g. `--skip-modelable` while migration is in progress).
- [ ] **2.2** Author `.mdl` for the `tracing` domain: canonical `Span`/`SpanEvent` entities + `auto projections … { db, reply }` (state explicitly if `request`/`event` are skipped and why).
- [ ] **2.3** Generate Rust artifacts; replace `libs/domain/src/span.rs:5-137` (`Span`, `SpanRow`, `SpanEvent`, `SpanEventRow`, `SpanKind`, `StatusCode`, `From` impls) with generated types + generated conversions; preserve the `#[cfg(feature = "storage")]` boundary.
- [ ] **2.4** Replace `services/query-api/src/traces.rs:32-50` (`TraceResponse`, `FacetValue`, `TraceListResponse`) with generated `reply`-projection types where they map 1:1; keep handler-local aggregation types (`TraceHistogramResponse`, etc.) hand-written if they don't represent canonical domain data — note why in the PR.
- [ ] **2.5** Generate TypeScript and replace `apps/frontend/src/api/traces.ts:1-43` (`Span`, `SpanEvent`, `TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse`); confirm with `apps/frontend/package.json` how generated output is wired into the build (new `generated/` import path vs. existing conventions).
- [ ] **2.6** Full verification: Rust tests, frontend typecheck/build/test, `scripts/local-ci.sh`. Confirm wire-format equivalence — this is a source-of-truth change, not a behavior change.
- [ ] **2.7** Write up the pattern as a short runbook addendum to `docs/agent-context.md` so Phase 3 domains can follow it mechanically.

## Phase 3 — Full type-by-type migration (this repo, one small PR per domain)

Each follows the Phase 2 template (define `.mdl` → generate → replace hand-written Rust+Row+API+TS → verify → delete dead code). Ordered simplest-first:

- [ ] **3.1 Logs** — `libs/domain/src/log.rs:6-70` (`LogRecord`/`LogRow`), `services/query-api/src/logs.rs:16-41`, `apps/frontend/src/api/logs.ts:5-36`
- [ ] **3.2 Metrics** — `libs/domain/src/metric.rs:6-150` (`MetricSeries`/`MetricPoint`/Rows), `services/query-api/src/metrics.rs:14-72` (incl. `MetricCatalogRow`→`MetricCatalogEntry` and `MetricGroupPointRow`→`MetricPoint` conversions at lines 309, 326), `apps/frontend/src/api/metrics.ts:5-37`
- [ ] **3.3 Notifications** — `services/query-api/src/notifications.rs:11-55` (incl. `From` impl at line 43), `apps/frontend/src/api/notifications.ts:5-19`
- [ ] **3.4 Admin/Members** — `services/query-api/src/admin_members.rs:25-45`, `apps/frontend/src/api/admin-members.ts:9-21`
- [ ] **3.5 Schemas/SLOs** — `services/query-api/src/schemas.rs:30-68`, `services/query-api/src/slos.rs:9-37`, `apps/frontend/src/api/slos.ts:5-25`
- [ ] **3.6 Incidents** — `services/query-api/src/incidents.rs:13-69` (incl. `IncidentRow`/`IncidentDetailRow`), `apps/frontend/src/api/incidents.ts:5-26`
- [ ] **3.7 Alerts** — `services/query-api/src/alerts.rs:15-95` (incl. `AlertRuleDetailRow`/`FiringRow`), `apps/frontend/src/api/alerts.ts:5-77`
- [ ] **3.8 Dashboards** — `services/query-api/src/dashboards.rs:17-67` (incl. `GrantItem` `sqlx::FromRow`), `apps/frontend/src/api/dashboards.ts:7-71`
- [ ] **3.9 NLQ/Visualization** — `libs/domain/src/{nlq,visualization,envelope}.rs`, `services/query-api/src/mcp_tools.rs:37-80` (incl. `From` impls at lines 120-174), `apps/frontend/src/api/nlq.ts:3-76`. Likely the most complex (CEL-computed fields, `NlqResponse` union type) — evaluate whether modelable's projection model can represent it before committing to full replacement; otherwise document as a deliberate, named exception.

**Per-domain rule:** only replace types that represent canonical domain/wire contracts. Handler-local validation-only shapes (`*Params`, ad-hoc histogram buckets, etc.) may stay hand-written — state the reason in each PR rather than force-fitting them into the model.

## Phase 4 — Cleanup & documentation

- [ ] Remove dead hand-written struct/interface definitions and `From`/`Into` impls once each domain's generated code is verified in place.
- [ ] Update `docs/agent-context.md` with the generated-types convention and where `.mdl` sources live.
- [ ] Add an ADR documenting modelable's adoption as the type-mapping source of truth (`AGENTS.md` "ADR and Spec Synchronization" — this is an architecture/data-model change).
- [ ] Cross-reference `spec/adr/ADR-030-timestamp-representation.md` if generated types change how that convention is expressed/enforced.

---

## Verification (every step)

- `bash scripts/local-ci.sh` before every push (frontend typecheck/lint/build/test, Docker image build incl. Rust fmt/clippy/unit tests, smoke test) — mandatory per `AGENTS.md`.
- `cargo fmt --all` before staging any `.rs` change.
- modelable side: `uv run pytest tests/ -q` and `uv run modelable validate`/`compile` against new `.mdl` sources before tagging a release.
- Each domain migration PR must demonstrate before/after wire-format equivalence (no behavioral change) via existing integration tests (`services/query-api/tests/http_api_integration.rs`) and frontend component tests.
- **Lineage proof, per migrated domain:** run `modelable lineage <domain.Model@version>` (and `modelable inspect … --auto`) and paste the result in the PR description — this is the concrete evidence that "types are fully tracked" rather than just "types are generated." A domain isn't done until its lineage report shows every field tracing back to its canonical source with no `type_loss` warnings from the emitters.
