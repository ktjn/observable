# P4-S8 Load, Chaos, Tenant-Escape, and Upgrade/Rollback Readiness Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package the existing release-readiness evidence into a repeatable local gate and add one focused chaos probe that proves the platform recovers after a single workload disruption without losing tenant isolation or rollback behavior.

**Architecture:** Keep this slice in shell orchestration only. Reuse the existing perf smoke, tenant-escape smoke, and kind rollback checks instead of rewriting them, then add one narrow failure-injection script that restarts a single compose service while the pipeline is live. The new umbrella runner should be the only entry point a reviewer needs to run for P4-S8 evidence; it should call the existing scripts in a fixed order and fail fast with a clear exit status.

**Tech Stack:** Bash, Docker Compose, kind, kubectl, curl, jq.

---

### Task 1: Add a focused chaos probe that restarts one runtime service during live traffic

**Files:**
- Create: `scripts/chaos-smoke.sh`
- Create: `tests/e2e/chaos_smoke_unit.sh`

**Acceptance target:** One repeatable failure-injection script can stop and restart a single compose service, then prove the ingest/query pipeline recovers and tenant isolation still fails closed.

- [ ] **Step 1: Write the failing unit test**

Create `tests/e2e/chaos_smoke_unit.sh` in the same style as `tests/e2e/smoke_test_unit.sh`. The test should:
- source `scripts/chaos-smoke.sh` in helper-only mode without executing the main flow;
- assert that the script exports its helper functions or main entrypoint without side effects;
- stub `docker compose` so the test can verify the restart target and command order;
- stub `curl`/`jq` so the helper can prove it waits for a previously ingested trace/log/metric to become queryable again after the restart.

Expected command:

```bash
bash tests/e2e/chaos_smoke_unit.sh
```

Expected result: the test prints a single `PASS:` line and exits 0.

- [ ] **Step 2: Run the unit test and confirm it fails for the right reason**

Run:

```bash
bash tests/e2e/chaos_smoke_unit.sh
```

Expected: fail because `scripts/chaos-smoke.sh` does not yet exist.

- [ ] **Step 3: Implement the minimal chaos probe**

Implement `scripts/chaos-smoke.sh` so it:
- uses the same local dev tenant and API key values as `tests/e2e/smoke_test.sh`;
- starts from the existing compose stack, or brings it up with `docker compose up -d --wait` if it is not already running;
- ingests one trace, one log, and one metric using the existing OTLP HTTP endpoint;
- confirms the same cross-tenant denial behavior already covered by `tests/e2e/smoke_test.sh`;
- restarts exactly one service that can disrupt the pipeline in a visible way, `storage-writer` first choice;
- waits for the restarted service to become healthy again;
- re-queries the previously ingested data and exits non-zero if any signal does not recover.

Prefer reusing helpers from `tests/e2e/smoke_test.sh` by loading them in source-only mode instead of duplicating retry logic. Keep the failure injection to one service restart only; do not add a broad chaos harness in this slice.

- [ ] **Step 4: Run the unit test and confirm it passes**

Run:

```bash
bash tests/e2e/chaos_smoke_unit.sh
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/chaos-smoke.sh tests/e2e/chaos_smoke_unit.sh
git commit -m "test: add chaos smoke probe"
```

---

### Task 2: Add a release-candidate umbrella runner for P4-S8

**Files:**
- Create: `scripts/release-candidate-suites.sh`
- Create: `tests/e2e/release_candidate_suites_unit.sh`

**Acceptance target:** One top-level script can run the load baseline, tenant-escape smoke, chaos probe, and upgrade/rollback evidence in a deterministic order with clear failure reporting.

- [ ] **Step 1: Write the failing unit test**

Create `tests/e2e/release_candidate_suites_unit.sh` to verify:
- the runner invokes `docker compose up perf-smoke --abort-on-container-exit`;
- the runner invokes `docker compose up smoke-test --abort-on-container-exit`;
- the runner invokes `scripts/chaos-smoke.sh`;
- the runner invokes `scripts/kind-test.sh` for upgrade/rollback evidence;
- failures propagate immediately instead of being masked.

Expected command:

```bash
bash tests/e2e/release_candidate_suites_unit.sh
```

Expected result: fail until `scripts/release-candidate-suites.sh` exists.

- [ ] **Step 2: Run the unit test and confirm it fails for the right reason**

Run:

```bash
bash tests/e2e/release_candidate_suites_unit.sh
```

Expected: fail because the runner script is missing.

- [ ] **Step 3: Implement the umbrella runner**

Implement `scripts/release-candidate-suites.sh` as a thin orchestrator that:
- checks for `docker`, `bash`, `curl`, `jq`, `kind`, `kubectl`, and `helm` where those tools are needed by the underlying scripts;
- runs the load baseline through the existing compose perf service;
- runs the tenant-escape smoke through the existing compose smoke service;
- runs the new chaos probe from Task 1;
- runs the existing `scripts/kind-test.sh` rollback verification last so its full cluster teardown stays isolated from the compose-based checks;
- prints a short PASS/FAIL summary for each stage and exits on the first failure.

Do not duplicate any of the existing ingest/query/assert logic inside the runner. The runner should only sequence the gates and surface failures with enough context for a human to replay the failing stage directly.

- [ ] **Step 4: Run the unit test and confirm it passes**

Run:

```bash
bash tests/e2e/release_candidate_suites_unit.sh
```

Expected: PASS.

- [ ] **Step 5: Run the individual gates in their real mode**

Run:

```bash
docker compose up perf-smoke --abort-on-container-exit
docker compose up smoke-test --abort-on-container-exit
bash scripts/chaos-smoke.sh
bash scripts/kind-test.sh
```

Expected: each gate exits 0 on its own, and the kind test still proves rollback by moving to revision 2 and back to the previous revision.

- [ ] **Step 6: Commit**

```bash
git add scripts/release-candidate-suites.sh tests/e2e/release_candidate_suites_unit.sh
git commit -m "test: add release candidate suite runner"
```

---

### Task 3: Update the active testing guidance and roadmap evidence

**Files:**
- Modify: `spec/11-testing.md`
- Modify: `docs/agent-context.md`
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`

**Acceptance target:** The repo docs name the new release-candidate runner, describe the chaos probe, and mark P4-S8 complete with the exact verification used.

- [ ] **Step 1: Add the new gate to the testing spec**

Update `spec/11-testing.md` so the release-candidate section points at the new umbrella runner and names the chaos probe as the missing failure-injection evidence for P4-S8. Keep the existing meaning of the nightly and release-candidate gates intact; this slice should clarify the execution path, not redefine the policy.

- [ ] **Step 2: Add the new operational note to agent context**

Update `docs/agent-context.md` with one short note that the P4-S8 readiness gate now lives in `scripts/release-candidate-suites.sh`, and that the chaos probe is the single-service restart check in `scripts/chaos-smoke.sh`.

- [ ] **Step 3: Mark the roadmap slice complete**

Update `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` to:
- mark `P4-S8` complete;
- record the exact scripts used for load, tenant-escape, chaos, and rollback evidence;
- add the checkpoint answer for the slice;
- keep the next-slice guidance accurate after P4-S8 closes.

- [ ] **Step 4: Archive this detailed plan**

When the implementation and docs agree, move this file to `archived/plans/` in the same iteration and update any active links that pointed here.

- [ ] **Step 5: Run the final verification set**

Run:

```bash
bash -n scripts/chaos-smoke.sh
bash -n scripts/release-candidate-suites.sh
bash tests/e2e/chaos_smoke_unit.sh
bash tests/e2e/release_candidate_suites_unit.sh
docker compose up perf-smoke --abort-on-container-exit
docker compose up smoke-test --abort-on-container-exit
bash scripts/chaos-smoke.sh
bash scripts/kind-test.sh
bash scripts/local-ci.sh
```

If `docker`, `kind`, or `helm` are unavailable in the current shell, use the narrowest documented `scripts/local-ci.sh` skip flags that still exercise the touched shell code, and record the skipped surface explicitly in the PR.

- [ ] **Step 6: Commit**

```bash
git add spec/11-testing.md docs/agent-context.md docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md archived/plans/2026-05-22-p4-s8-load-chaos-tenant-escape-upgrade-rollback.md
git commit -m "docs: close p4-s8 readiness gate"
```

---

**Roll-forward path:** Keep the new scripts as the canonical local release-readiness entry points.
**Rollback path:** Revert the two new shell scripts and the doc references; the existing `scripts/perf-smoke.sh`, `tests/e2e/smoke_test.sh`, and `scripts/kind-test.sh` continue to provide the underlying signals.
**Telemetry impact:** None on product telemetry; the new chaos script only restarts one compose service in the local verification environment.
**Auth/tenancy impact:** No new auth model or tenancy behavior; the chaos probe must continue to prove cross-tenant denial with the existing smoke assertions.
**Data retention or migration impact:** None.
**ADR/spec sync:** No new ADR required. This slice operationalizes the already-accepted release-readiness gates without changing deployment, data, or security architecture.
