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

## Resolved Decisions Before Phase 2 Starts

1. **Generated-artifact commit strategy: Option (a) (Commit generated `.rs`/`.ts` files).**
   - **Decision:** Generated code is committed alongside its `.mdl` sources. Regeneration is done manually/automatically via `modelable compile` and verified as clean in CI via a diff check (`git diff --exit-code`).
   - **Reasoning:** Avoids forcing a Python+uv dependency on standard Rust/TypeScript compilation steps, keeps PR diffs inspectable for changes to generated types, and ensures IDE/LSP import resolution works out-of-the-box without requiring a pre-build.
2. **Python/uv as a CI-gating dependency.**
   - **Decision:** Python/uv/modelable is only a toolchain dependency for developers editing models or running local CI validations. We will add a check in `scripts/local-ci.sh` that skips or warns if python/uv is missing, unless `.mdl` files themselves are modified in the change, in which case it is a hard gate.
   - **Reasoning:** Prevents local development friction for unrelated changes (like CSS/Rust handler updates) while maintaining strict correctness for data-model changes.

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

- [x] **2.1** Pin modelable as a dev dependency per the modelable repo's `docs/consuming-modelable.md`; wire `modelable validate`/`compile` into `scripts/local-ci.sh` (gated, e.g. `--skip-modelable` while migration is in progress). *(Done in PR #399, but `models/requirements.txt` pins `modelable==0.2.0` — must be bumped to `0.2.1` as part of 2.3 below; `tracing.mdl`'s `@wire(json.case: ...)` on enums and entity→projection `@wire` hint inheritance require modelable PR #45, which only shipped in 0.2.1.)*
- [x] **2.2** Author `.mdl` for the `tracing` domain: canonical `Span`/`SpanEvent` entities + `db` projections (`SpanRow`, `SpanEventRow`). *(Done in PR #399 as `models/tracing.mdl`. `reply`-projection types for `TraceResponse`/`FacetValue`/`TraceListResponse` are NOT modeled — deferred to a future step per the scope note in 2.3 below.)*
- [x] **2.3** Generate Rust `db`-projection types only; replace `SpanRow`/`SpanEventRow` in `libs/domain/src/span.rs` with generated `TracingSpanRowV1`/`TracingSpanEventRowV1`, with hand-rewritten `From`/`Into` conversions to the existing hand-written `Span`/`SpanEvent`/`SpanKind`/`StatusCode` (which remain hand-written — see scope note below). Preserve the `#[cfg(feature = "storage")]` boundary.

  **Scope note (narrowed from the original "replace `span.rs:5-137` wholesale" framing):** Exploration of the actual generated output showed the canonical entities `TracingSpanV1`/`TracingSpanEventV1` are not drop-in replacements for hand-written `Span`/`SpanEvent` — enums (`SpanKind`/`StatusCode`) are generated as plain `String`, the `attributes`/`resourceAttributes` maps are generated as `String` (JSON-encoded), and `Span.events: Vec<SpanEvent>` (nested events) has no generated equivalent. Only the `db`-projection Row types (`TracingSpanRowV1`/`TracingSpanEventRowV1`), which are structurally near-identical to the hand-written `SpanRow`/`SpanEventRow`, are replaced. `Span`, `SpanEvent`, `SpanKind`, `StatusCode` and the conversion *logic* (enum↔string, HashMap↔JSON-string) remain hand-written — only the Row struct *shapes* and their derives become generated.

  **Bump dependency:** update `models/requirements.txt` to `modelable==0.2.1` (see 2.1 note).

  **Generated file location:** `libs/domain/src/generated/tracing/` (new directory, part of the `domain` crate, not gitignored — `dist/` is gitignored so that path from the `requirements.txt` usage comment is not used). Commit all four files modelable emits for the domain (`tracing_span_v1.rs`, `tracing_span_event_v1.rs`, `tracing_span_row_v1.rs`, `tracing_span_event_row_v1.rs`) plus a `mod.rs` re-exporting them with `#![allow(dead_code)]` (the two canonical-entity files and their trivial `From<TracingSpanV1> for TracingSpanRowV1`-style impls are unused by Observable but kept un-edited so regeneration stays a clean diff). `lib.rs` gains a private `mod generated;`.

  **Type aliases:** `pub type SpanRow = generated::tracing::TracingSpanRowV1;` and `pub type SpanEventRow = generated::tracing::TracingSpanEventRowV1;` in `span.rs`. Field names match exactly (snake_case, same names), so all existing struct-literal usages across `services/storage-writer`, `services/query-api`, and integration tests keep compiling unchanged.

  **⚠️ `SELECT_COLS` field-order break:** The generated `TracingSpanRowV1` moves `parent_span_id` from position 4 (current order, matching `migrations/clickhouse/001_create_spans.sql`) to the *last* field. ClickHouse `Row`-derive `INSERT` is name-based (safe either way), but `fetch_all::<SpanRow>()` decodes RowBinary *positionally* against the `SELECT_COLS` list. **`services/query-api/src/traces.rs:81-85` (`SELECT_COLS`) must be updated to move `parent_span_id` to the end** to match the new struct order. `SpanEventRow`'s field order is unchanged — no `SELECT_COLS` change needed for `span_events`.

  **SQL DDL:** generated DDL is not adopted in this step (lacks `observable.` prefix, `DEFAULT`s, `INDEX`/`PARTITION BY`/tuned `ORDER BY`/`TTL`/`SETTINGS`) — `migrations/clickhouse/001_create_spans.sql` stays hand-written. Flagged as a known gap/follow-up, not a blocker.

  **Verification:** `cargo fmt --all`; `cargo test` for `domain`, `query-api`, `storage-writer` (with `storage` feature); `bash scripts/local-ci.sh`; `modelable lineage tracing.SpanRow@1` and `tracing.SpanEventRow@1` output pasted into the PR description; confirm `services/query-api/tests/http_api_integration.rs` and `clickhouse_integration.rs` still pass (real insert/select round-trips against `spans`).
- [x] **2.4** `TraceResponse` (`{trace_id, spans: Vec<Span>, events: Vec<SpanEvent>}`), `FacetValue`, and `TraceListResponse` in `services/query-api/src/traces.rs` are handler-level aggregation/wrapper types with no 1:1 generated entity or projection equivalent — per the Phase 3 "per-domain rule," they remain hand-written. The concrete 2.4 deliverable was closing the `attributes`/`resourceAttributes` representation gap in `tracing.mdl` (now `map<string, json>`, generating `HashMap<String, serde_json::Value>` for `TracingSpanV1`/`TracingSpanEventV1`), enabled by modelable v0.3.0's `json` type — see `docs/superpowers/specs/2026-06-12-tracing-attributes-json-type-design.md`. Full entity-level generation of `Span`/`SpanEvent` remains blocked on generating Rust enums (`SpanKind`/`StatusCode`) and nested types (`Span.events: Vec<SpanEvent>`), per the step 2.3 scope note.
- [x] **2.5** Generated TypeScript for `tracing.Span@1`/`tracing.SpanEvent@1` (enabled by modelable v0.4.0's `@wire(json.fieldCase: "snake_case")` hint — see `docs/superpowers/specs/2026-06-13-tracing-typescript-field-case-design.md`) and committed it under `apps/frontend/src/api/generated/tracing/`. `apps/frontend/src/api/traces.ts`'s `Span`/`SpanEvent` interfaces are now re-exports of the generated types. `TraceResponse`, `FacetValue`, `Facets`, `TraceListResponse` remain hand-written — same rationale as 2.4 (handler-level aggregation/wrapper types with no 1:1 generated equivalent).
- [x] **2.6** Full verification: Rust tests, frontend typecheck/build/test, `scripts/local-ci.sh`. Confirm wire-format equivalence — this is a source-of-truth change, not a behavior change. *(Done as part of 2.5 Task 5, PR #403: `modelable compile --target rust` diff-checked as unchanged vs. `libs/domain/src/generated/tracing/`, `npm run typecheck`/`npm test` (424 tests)/`npm run build` all pass, `bash scripts/local-ci.sh` cargo tests pass. The frontend lint failure surfaced by `local-ci.sh` is pre-existing and unrelated — see `apps/frontend/src/features/onboarding/OnboardingWizard.tsx`.)*
- [x] **2.7** Write up the pattern as a short runbook addendum to `docs/agent-context.md` so Phase 3 domains can follow it mechanically. *(Added "Modelable Type-Mapping Migration" section to `docs/agent-context.md`.)*

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
