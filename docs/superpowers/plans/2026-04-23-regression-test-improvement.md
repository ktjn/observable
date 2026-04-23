# Regression Test Improvement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Status:** Completed on `main` across PRs #102-#106. The checklist below is kept as the implementation record.

**Goal:** Make the regression gate more deterministic, better scoped, and harder to weaken while preserving the existing build and end-to-end functionality.

**Architecture:** Keep `scripts/local-ci.sh` as the mandatory pre-push gate and keep Docker Compose as the local integration environment. Improve the existing smoke and performance scripts incrementally with preflight checks, unique test data, bounded polling, diagnostics, and explicit no-regression reporting before adding broader coverage.

**Tech Stack:** Bash, Docker Compose, Rust workspace checks, npm frontend checks, `curl`, `jq`, `grpcurl`, and the existing Rust/React test suites.

---

## Current State Review

### Existing Regression Gate

- `scripts/local-ci.sh` is the mandatory code-change gate before push. It runs Rust format, clippy, Rust tests, frontend typecheck/lint/build/tests, Docker image build, frontend image build, and the Compose `smoke-test` service unless skipped by flags.
- `tests/e2e/smoke_test.sh` is the main end-to-end regression test. It currently verifies:
  - OTLP HTTP trace ingest returns success.
  - Query API returns the ingested trace by trace ID.
  - Trace search returns the smoke service.
  - OTLP HTTP log and metric ingest return success.
  - OTLP gRPC log ingest returns success and lands in ClickHouse.
  - Service discovery includes `smoke-svc`.
- `scripts/perf-smoke.sh` is an extended performance smoke gate. It samples ingest and query endpoints, reports P50/P95, and fails on threshold breaches. It is wired as a Compose `perf-smoke` service, but it is not part of `scripts/local-ci.sh`.
- `spec/11-testing.md` already defines no-regression rules, required PR verification blocks, and the performance smoke baseline.
- `spec/10-process.md` requires agents to verify that no new errors, warnings, regressions, flaky tests, type errors, lint findings, generated-code drift, policy violations, or documentation check failures are introduced.

### Risks and Gaps

- The smoke script uses fixed IDs and service names for some paths. Repeated local runs can observe older rows and hide pipeline regressions.
- The smoke script uses fixed sleeps instead of bounded polling. Slow-but-working runs can fail, and broken runs provide limited diagnostics.
- Required tool preflights are implicit. Missing `jq`, `grpcurl`, Docker, or Compose support can fail late with unclear output.
- The smoke test mostly covers positive paths. It does not yet cover tenant-cross-read denial, viewer ingest denial, malformed payload rejection, or missing-auth rejection.
- The local mandatory gate does not run `perf-smoke`; that is reasonable for speed, but agent instructions need to make clear when performance-sensitive changes must run it.
- There is no regression-gate change policy that forbids weakening, deleting, skipping, or broadening skip flags without a compensating signal and reviewer approval.
- Diagnostics on failure are not collected automatically from the relevant Compose services.

### Non-Negotiables

- Do not remove, skip, or weaken an existing regression assertion unless the PR includes a replacement assertion, linked issue, owner, expiry date, and explicit reviewer approval.
- Do not make `scripts/local-ci.sh` less strict for code changes.
- Do not add broad new work to the mandatory gate unless the runtime cost and flake risk are measured.
- Every implementation slice must leave the build and existing functionality working.
- Documentation-only changes remain exempt from `scripts/local-ci.sh`, but must still pass Markdown/spec review.

---

## File Structure

- Modify: `tests/e2e/smoke_test.sh` for deterministic data, bounded polling, preflight checks, and targeted negative-path assertions.
- Modify: `scripts/local-ci.sh` only when a gate is added or renamed; preserve existing mandatory checks and skip flag semantics.
- Modify: `scripts/perf-smoke.sh` only for diagnostics, determinism, or endpoint coverage that does not change the published thresholds without spec updates.
- Modify: `docker-compose.yml` only when adding a new verification service or adjusting smoke/perf service environment.
- Modify: `spec/11-testing.md` when the test strategy, gate scope, thresholds, or regression policy changes.
- Modify: `spec/10-process.md`, `AGENTS.md`, `AGENT.md`, `CLAUDE.md`, and `GEMINI.md` when agent instructions change.

---

## Task 1: Capture the Existing Regression Contract

**Files:**
- Modify: `spec/11-testing.md`
- Test: documentation review

- [x] **Step 1: Add an explicit regression contract section**

Add a short subsection under `18.6 Per-Iteration No-Regression Rules` that states the current mandatory smoke coverage:

```markdown
**Current mandatory smoke coverage**

`scripts/local-ci.sh` runs `docker compose up smoke-test --abort-on-container-exit`
for code changes unless Docker or smoke tests are explicitly skipped. The smoke
test must continue to prove at least one successful path for trace ingest, trace
detail query, trace search, log ingest, metric ingest, OTLP gRPC ingest, and
service discovery.
```

- [x] **Step 2: Review the diff**

Run:

```bash
git diff -- spec/11-testing.md
```

Expected: the diff only documents the existing contract and does not change test behavior.

- [x] **Step 3: Commit**

Run:

```bash
git add spec/11-testing.md
git commit -m "Document current regression smoke contract"
```

Expected: commit succeeds on the feature branch.

---

## Task 2: Make Smoke Runs Deterministic

**Files:**
- Modify: `tests/e2e/smoke_test.sh`
- Test: `docker compose up smoke-test --abort-on-container-exit`

- [x] **Step 1: Add per-run IDs and names**

Replace fixed smoke identifiers with a run suffix derived from nanoseconds:

```bash
RUN_ID="${RUN_ID:-$(date +%s%N)}"
SERVICE_NAME="smoke-svc-${RUN_ID}"
GRPC_SERVICE_NAME="smoke-grpc-svc-${RUN_ID}"
TRACE_ID="$(printf '%032x' "$((RUN_ID % 4294967295))")"
```

Use `SERVICE_NAME`, `GRPC_SERVICE_NAME`, and `TRACE_ID` in every ingest and query payload.

- [x] **Step 2: Run the smoke test**

Run:

```bash
docker compose up smoke-test --abort-on-container-exit
```

Expected: the smoke-test container exits 0 and every query observes only rows from the current run.

- [x] **Step 3: Run the mandatory gate**

Run:

```bash
bash scripts/local-ci.sh
```

Expected: all checks pass.

- [x] **Step 4: Commit**

Run:

```bash
git add tests/e2e/smoke_test.sh
git commit -m "Make smoke test data unique per run"
```

Expected: commit succeeds after verification passes.

---

## Task 3: Replace Fixed Sleeps with Bounded Polling

**Files:**
- Modify: `tests/e2e/smoke_test.sh`
- Test: `docker compose up smoke-test --abort-on-container-exit`

- [x] **Step 1: Add a bounded polling helper**

Add:

```bash
wait_for_json_count() {
  local label="$1"
  local url="$2"
  local jq_expr="$3"
  local attempts="${4:-20}"
  local delay_seconds="${5:-1}"
  local result count

  for _ in $(seq 1 "$attempts"); do
    result=$(curl -sf -H "X-Tenant-ID: $TENANT_ID" "$url" || true)
    count=$(echo "$result" | jq -r "$jq_expr" 2>/dev/null || echo 0)
    if [ "$count" -gt 0 ]; then
      echo " OK ($label) - $count record(s)"
      return 0
    fi
    sleep "$delay_seconds"
  done

  echo " FAIL: $label did not return records after $attempts attempts"
  echo " Last result: ${result:-<empty>}"
  return 1
}
```

- [x] **Step 2: Use the helper for trace, search, gRPC log, and discovery checks**

Replace fixed `sleep 3` waits and one-shot query assertions with bounded polling. Keep the same positive coverage.

- [x] **Step 3: Verify**

Run:

```bash
docker compose up smoke-test --abort-on-container-exit
bash scripts/local-ci.sh
```

Expected: both commands pass. If the smoke test fails, it reports the last response body for the failed check.

- [x] **Step 4: Commit**

Run:

```bash
git add tests/e2e/smoke_test.sh
git commit -m "Use bounded polling in smoke test"
```

Expected: commit succeeds after verification passes.

---

## Task 4: Add Tool Preflight Checks

**Files:**
- Modify: `tests/e2e/smoke_test.sh`
- Modify: `scripts/perf-smoke.sh`
- Test: direct script syntax and Compose smoke/perf services

- [x] **Step 1: Add a shared local preflight pattern to each script**

Add near the top of each script:

```bash
require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available"
    exit 1
  fi
}

require_command curl
require_command jq
```

Add `require_command grpcurl` only to `tests/e2e/smoke_test.sh`.

- [x] **Step 2: Verify syntax**

Run:

```bash
bash -n tests/e2e/smoke_test.sh
bash -n scripts/perf-smoke.sh
```

Expected: both commands exit 0.

- [x] **Step 3: Verify behavior**

Run:

```bash
docker compose up smoke-test --abort-on-container-exit
docker compose up perf-smoke --abort-on-container-exit
```

Expected: both services exit 0 in a healthy local stack.

- [x] **Step 4: Commit**

Run:

```bash
git add tests/e2e/smoke_test.sh scripts/perf-smoke.sh
git commit -m "Add smoke script preflight checks"
```

Expected: commit succeeds after verification passes.

---

## Task 5: Add Minimal Negative-Path Regression Coverage

**Files:**
- Modify: `tests/e2e/smoke_test.sh`
- Test: `docker compose up smoke-test --abort-on-container-exit`

- [x] **Step 1: Add missing-auth rejection check**

Add after the positive trace ingest:

```bash
echo "Checking missing auth rejection..."
AUTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$INGEST/v1/traces" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[]}")
if [ "$AUTH_STATUS" = "401" ]; then
  echo " OK (missing auth rejected)"
else
  echo " FAIL: missing auth returned HTTP $AUTH_STATUS"
  exit 1
fi
```

- [x] **Step 2: Add cross-tenant trace denial check**

Query the current run trace with a different tenant ID and assert the response does not expose spans:

```bash
OTHER_TENANT_ID="00000000-0000-0000-0000-000000000002"
CROSS_RESULT=$(curl -sf -H "X-Tenant-ID: $OTHER_TENANT_ID" "$QUERY/v1/traces/$TRACE_ID" || true)
CROSS_SPAN_COUNT=$(echo "$CROSS_RESULT" | jq '.spans | length' 2>/dev/null || echo 0)
if [ "$CROSS_SPAN_COUNT" -eq 0 ]; then
  echo " OK (cross-tenant trace hidden)"
else
  echo " FAIL: cross-tenant query exposed $CROSS_SPAN_COUNT span(s)"
  echo " Result: $CROSS_RESULT"
  exit 1
fi
```

- [x] **Step 3: Verify**

Run:

```bash
docker compose up smoke-test --abort-on-container-exit
bash scripts/local-ci.sh
```

Expected: both commands pass, and the smoke output includes missing-auth and cross-tenant checks.

- [x] **Step 4: Commit**

Run:

```bash
git add tests/e2e/smoke_test.sh
git commit -m "Add negative smoke regression checks"
```

Expected: commit succeeds after verification passes.

---

## Task 6: Add Failure Diagnostics Without Changing Pass Criteria

**Files:**
- Modify: `scripts/local-ci.sh`
- Test: `bash scripts/local-ci.sh`

- [x] **Step 1: Add Compose log capture on smoke failure**

Change only the smoke-test failure branch so a failed smoke run prints recent logs for `ingest-gateway`, `query-api`, `stream-processor`, and `storage-writer` before exiting non-zero.

```bash
if docker compose up smoke-test --abort-on-container-exit; then
  ok "smoke-test"
else
  docker compose logs --no-color --tail=120 ingest-gateway query-api stream-processor storage-writer || true
  fail "smoke-test"
fi
```

- [x] **Step 2: Verify**

Run:

```bash
bash scripts/local-ci.sh
```

Expected: all checks pass. On an intentionally broken local stack, the script must still exit non-zero and include recent service logs.

- [x] **Step 3: Commit**

Run:

```bash
git add scripts/local-ci.sh
git commit -m "Print service diagnostics on smoke failure"
```

Expected: commit succeeds after verification passes.

---

## Task 7: Update Agent Instructions and PR Policy

**Files:**
- Modify: `AGENTS.md`
- Modify: `AGENT.md`
- Modify: `CLAUDE.md`
- Modify: `GEMINI.md`
- Modify: `spec/10-process.md`
- Test: documentation review

- [x] **Step 1: Add regression-gate policy to all agent instruction files**

Add a section requiring agents to:

```markdown
## Regression Gate Stewardship

- Treat `scripts/local-ci.sh`, `tests/e2e/smoke_test.sh`, `scripts/perf-smoke.sh`, and Compose verification services as protected regression gates.
- Before changing a regression gate, state the current coverage it provides and the exact coverage that will exist after the change.
- Never delete, weaken, skip, or quarantine a regression assertion unless the PR includes a replacement signal, linked issue, owner, expiry date, and explicit reviewer approval.
- Regression-gate changes must preserve existing build and product functionality. Run the narrowest affected check first, then the required local gate for the touched surface.
- Performance-sensitive changes must run `docker compose up perf-smoke --abort-on-container-exit` or explain why the performance gate is not relevant.
```

- [x] **Step 2: Mirror the policy in `spec/10-process.md`**

Add the same requirements under `16.7 AI Agent Guidance`, phrased as official process.

- [x] **Step 3: Review docs**

Run:

```bash
git diff -- AGENTS.md AGENT.md CLAUDE.md GEMINI.md spec/10-process.md
```

Expected: all agent instruction files carry consistent regression-gate policy.

- [x] **Step 4: Commit**

Run:

```bash
git add AGENTS.md AGENT.md CLAUDE.md GEMINI.md spec/10-process.md
git commit -m "Add regression gate stewardship instructions"
```

Expected: commit succeeds.

---

## Task 8: Document Extended Gate Usage

**Files:**
- Modify: `spec/11-testing.md`
- Test: documentation review

- [x] **Step 1: Clarify when to run `perf-smoke`**

Add:

```markdown
Run `docker compose up perf-smoke --abort-on-container-exit` for changes that can
affect ingest latency, query latency, dashboard load, ClickHouse query shape,
storage writes, stream processing, Docker resource limits, or service startup
ordering. If skipped, the PR must explain why performance is not in scope.
```

- [x] **Step 2: Verify documentation consistency**

Run:

```bash
git diff -- spec/11-testing.md
```

Expected: the testing spec remains consistent with `scripts/perf-smoke.sh` thresholds.

- [x] **Step 3: Commit**

Run:

```bash
git add spec/11-testing.md
git commit -m "Document performance smoke gate usage"
```

Expected: commit succeeds.

---

## Verification Plan for This Plan Document

This planning iteration is documentation-only. Required checks:

```bash
git diff --check
bash -n scripts/local-ci.sh
bash -n tests/e2e/smoke_test.sh
bash -n scripts/perf-smoke.sh
```

Expected:
- No whitespace errors.
- All Bash syntax checks exit 0.
- `scripts/local-ci.sh` is not required for this docs-only iteration by the root agent instructions.

ADR/spec synchronization:
- No ADR update is required for this plan because it does not change architecture, technology choice, deployment model, data model, security model, or roadmap scope. It documents process and proposes future regression-gate hardening aligned with ADR-019.
