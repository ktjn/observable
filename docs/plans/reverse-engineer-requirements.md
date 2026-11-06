# Plan: Reverse-Engineer Requirements, Tests and Risks into `spec/system.yaml`

**Status:** Slice 1 steps 0-2 done (both ingest-limit requirements `verified`, four gap risks recorded); step 3 next
**Method:** [system-spec/BROWNFIELD.md](../../system-spec/BROWNFIELD.md) Reverse Discovery steps 4-5; change flow in [AGENTS.md](../../AGENTS.md)
**Current spec:** 17 requirements, 7 risks, 29 evidence records (`spec/system.yaml`)

## Principles

- Record observed behavior, not intended behavior. Keep as-is gaps separate from target changes.
- One slice per PR, one component per slice. Each PR passes the release gate and records `--impact`/`--compat` output.
- Every new requirement gets evidence naming a real test (`path::test_name`) or an honest risk; no hand-written `passed` without a run.
- Tests are removed only with evidence they are redundant, and only with user approval (see step 3).
- Unreleased toolkit features (`REQ-08`, `--check-report`, `--emit-evidence`) are not relied on until a toolkit release is pinned.

## Slice order

1. Ingest limits (`observable-ingest`): rate limits, cardinality budgets (first slice, below).
2. Retention and purge (`admin-service`, `observable-control`).
3. Credential lifecycle and tenant isolation (`observable-auth`).
4. Usage report (`admin-service`).
5. Remaining 0.3/0.4 claims with executable checks (doctor, backup/restore drill, manifest compatibility).

## Slice 1: ingest limits

### Step 0: findings (done, from `services/ingest-gateway`)

| Observation | Evidence |
|---|---|
| Per-tenant rate limit applies to traces, logs and metrics over HTTP, gRPC and Prometheus Remote Write; HTTP returns 429 + `Retry-After: 1`, gRPC `ResourceExhausted` | `src/rate_limiter.rs`, `src/http-json/*.rs`, `src/grpc/*.rs`, `src/prometheus_rw/mod.rs` |
| Cardinality budget is a cumulative per-tenant count of submitted series, in memory per replica, never reset in production code; enforcement is off unless `METRIC_CARDINALITY_ENFORCE=true`; HTTP rejection returns `Retry-After: 60` | `src/cardinality.rs`, `src/main.rs:172-180` |
| `set_tenant_limit` and `set_tenant_budget` have no production caller, only tests, so per-tenant custom quotas cannot be configured at runtime | `grep set_tenant_` over `services/` |
| Limits key on tenant only; there is no per-environment limit | `check_key(&ctx.tenant_id)` |
| Tested: HTTP 429 for traces, logs, metrics; Prometheus RW rate limit and cardinality; HTTP and gRPC metric cardinality; per-tenant isolation (HTTP traces) | test names in `src/**` |
| Not tested: gRPC rate limit for traces, logs and metrics (closed in step 2) | no gRPC test asserts `rate limit exceeded` |
| Correction: production default rate limit is 100 requests/s per signal (`*_INGEST_RATE_LIMIT_PER_SECOND`); 1000 is the test stub. The exported counter is `ingest_gateway_rejections_total`. Defaults are untested config, so criteria omit them | `src/main.rs:160-178`, `src/observability.rs` |

### Step 1: requirements (spec edit, no code)

- Run `system-spec spec/system.yaml --impact ingest-otlp`; list affected items in the PR.
- Add requirements on `ingest-otlp`, as observed:
  - `req-ingest-rate-limit`: per-tenant, per signal, over every protocol; 429/`ResourceExhausted`; tenant isolation.
  - `req-ingest-cardinality-budget`: cumulative per-tenant series budget, enforced only when enabled; structured rejection and counter `ingest_rejections_total`.
- Status `specified` until evidence exists; promote to `verified` per requirement.
- Gate: `--stage verification`, then `--compat` against `main`.

### Step 2: bind tests and risks

- Evidence for each criterion that has a test (method `test`, provenance `path::test_name`); `--check-artifacts` must pass.
- Risks for the gaps in step 0, each with a remediation requirement and `expires` if waived:
  - gRPC rate limits unverified.
  - Custom per-tenant quota has no runtime configuration path (ROADMAP 0.5 wording claimed it; fixed).
  - Cardinality enforcement defaults off and the count never decays or resets.
  - No per-environment limits (ROADMAP 0.5 claimed bypass prevention across environments; fixed).
- Decide per gap whether it is a test to add (small PR), a roadmap wording fix, or an accepted residual risk. Do not weaken ROADMAP claims without user approval.

Result (done):

- Evidence: 14 `test` records, one per test, provenance `path::test_name`, each naming the criteria it covers. Added gRPC rate-limit tests for traces, logs and metrics, and rejection-counter asserts on the gRPC rate-limit and cardinality paths.
- `risk-ingest-grpc-rate-limit-unverified`: mitigation, closed by the new gRPC tests.
- `risk-ingest-tenant-quota-not-configurable`, `risk-ingest-cardinality-count-never-resets`, `risk-ingest-no-per-environment-limits`: `acceptance` with `residual_accepted`, pending user approval in review. A remediation requirement cannot sit in the spec unverified: release fails on STAT-04 and RISK-04. User approved acceptance and a ROADMAP.md 0.5 wording fix (done): the 0.5 paragraph no longer claims per-tenant custom limits or bypass prevention across environments, and lists the three accepted risks. Implementing a fix later switches its risk to `mitigation`.
- `verifies` tags and `--check-report` not used: unreleased toolkit feature, and `cargo test` emits no JUnit properties.
- Toolkit feedback (in-repo `system-spec/reference-implementation/metamodel.py`):
  - The CLI skips stage validation when `--check-artifacts` or `--check-drift` is given and those checks pass, so the AGENTS.md release gate never runs stage invariants. Run `--stage release` on its own as well.
  - A non-mandatory requirement marked `verified` with partial evidence passes release; STAT-04 checks status only and REQ-05 checks obligated requirements only.

### Step 3: obsolete-test audit (propose only)

- List candidate tests per slice with a reason and evidence: duplicate assertion already covered by a broader test, test for removed behavior, or test bound to no requirement and no unique regression value.
- Candidates found so far: none for deletion. Unit tests in `rate_limiter.rs` and `cardinality.rs` cover distinct logic from the HTTP tests and are not redundant.
- Weak test: `http-json/traces.rs::rate_limit_is_per_tenant` builds a separate `AppState` per tenant, so it cannot detect shared-quota leakage; isolation evidence uses `rate_limiter.rs::default_limit_applies_per_tenant` and `custom_tenant_rate_limit_isolated_in_shared_limiter` instead. Propose rewriting it against one shared state.
- Deletion needs explicit user approval per test; run the owning suite before and after and record the result.

### Step 4: close the slice

- Run `system-spec spec/system.yaml --stage release --check-artifacts --check-drift --root .`.
- Record gate results in the PR; update this plan's status; start the next slice.
