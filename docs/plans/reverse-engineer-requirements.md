# Plan: Reverse-Engineer Requirements, Tests and Risks into `spec/system.yaml`

**Status:** Slice 1 steps 0-1 done (requirements added as `specified`); step 2 next
**Method:** [system-spec/BROWNFIELD.md](../../system-spec/BROWNFIELD.md) Reverse Discovery steps 4-5; change flow in [AGENTS.md](../../AGENTS.md)
**Current spec:** 15 requirements, 3 risks, 15 evidence records (`spec/system.yaml`)

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
| Not tested: gRPC rate limit for traces, logs and metrics | no gRPC test asserts `rate limit exceeded` |

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
  - Custom per-tenant quota has no runtime configuration path (roadmap wording claims it).
  - Cardinality enforcement defaults off and the count never decays or resets.
  - No per-environment limits (roadmap exit evidence says "across tenants or environments").
- Decide per gap whether it is a test to add (small PR), a roadmap wording fix, or an accepted residual risk. Do not weaken ROADMAP claims without user approval.

### Step 3: obsolete-test audit (propose only)

- List candidate tests per slice with a reason and evidence: duplicate assertion already covered by a broader test, test for removed behavior, or test bound to no requirement and no unique regression value.
- Candidates found so far: none. Unit tests in `rate_limiter.rs` and `cardinality.rs` cover distinct logic from the HTTP tests and are not redundant.
- Deletion needs explicit user approval per test; run the owning suite before and after and record the result.

### Step 4: close the slice

- Run `system-spec spec/system.yaml --stage release --check-artifacts --check-drift --root .`.
- Record gate results in the PR; update this plan's status; start the next slice.
