# P4-S2 Backup And Restore Drill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Practice and time one restore path for the shared PostgreSQL control-plane dataset using a local Compose-backed logical backup/restore, then verify the restored data matches the source database for a representative table set.

**Architecture:** Keep the first restore drill hot-store-only because P4-S1 warm retention is deferred. Use the existing `postgres` container in Docker Compose and its built-in `pg_dump` / `psql` / `createdb` utilities rather than introducing a new backup service. The drill restores into a scratch database inside the same Postgres instance, compares source-versus-restore row counts for representative control-plane tables, and prints timings so operators can record RPO/RTO evidence.

**Tech Stack:** Bash, Docker Compose, PostgreSQL 17 client utilities, repo scripts, local CI.

---

## File Structure

- Create: `scripts/backup-restore-drill.sh`
- Modify: `docs/agent-context.md`
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` after the drill is implemented and verified
- No ADR change expected.
- No spec change expected; `spec/12-deployment.md` and `spec/11-testing.md` already require restore drills and release-candidate backup/restore coverage.

## Slice Contract

Source spec: `spec/10-process.md §16.4`, `spec/10-process.md §16.8`, `spec/11-testing.md §18.5`, `spec/11-testing.md §18.7`, `spec/11-testing.md §18.8`, `spec/12-deployment.md §19.5`, ADR-012, ADR-025.
Phase: 4.
Parent phase item: Add backup and restore drill for one dataset.
Acceptance target: one restore path is practiced and timed against the shared `observable` PostgreSQL dataset, the restored scratch database matches the source database for a representative control-plane table set, and the drill explicitly records that it is hot-store-only because P4-S1 warm retention is deferred.
User/operator outcome: operators can execute a repeatable local restore drill, capture timing evidence, and prove the hot control-plane dataset can be restored without depending on object storage.
Files or modules expected to change: `scripts/backup-restore-drill.sh`, `docs/agent-context.md`, and `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`.
Out of scope: warm-retention object-store restore, ClickHouse restore procedures, multi-dataset automation, scheduled production backup jobs, Kubernetes/kind changes, and any schema migration.
Verification: `bash -n scripts/backup-restore-drill.sh`, `docker compose up -d postgres postgres-setup`, `bash scripts/backup-restore-drill.sh`, `git diff --check`, and `bash scripts/local-ci.sh` before push because this is a code change.
Baseline: run `docker compose up -d postgres postgres-setup` first so the drill runs against the seeded local database rather than a blank instance.
New errors introduced: none.
Telemetry impact: the drill script emits start/end timing, source database, restore database, and table-validation results to stdout only; no service telemetry changes are introduced.
Auth/tenancy impact: none at the API layer. The drill exercises the shared control-plane database, so row-validation must cover tenant-scoped tables but does not change auth behavior.
Data retention or migration impact: no schema migration, no hot-row deletion, and no object-storage dependency. The scratch restore database is dropped at the end of the drill.
Rollback path: delete or stop using `scripts/backup-restore-drill.sh`; there is no persistent schema or runtime change to roll back.
ADR/spec sync: no ADR update required because the slice operationalizes existing restore-drill requirements without changing deployment, storage, or security architecture. No spec update required because the acceptance target is already covered by `spec/12-deployment.md` and `spec/11-testing.md`.
Checkpoint question: can the local operator prove one hot-store restore path, with timing and validation evidence, without object storage because P4-S1 remains deferred?
Next smallest slice: add a separate warm-retention boundary drill once P4-S1 exists, so the backup boundary includes object storage as well as hot storage.

---

### Task 1: Add The Drill Script Skeleton And CLI Contract

**Files:**
- Create: `scripts/backup-restore-drill.sh`
- Test: `bash -n scripts/backup-restore-drill.sh`

- [ ] **Step 1: Write the failing syntax check first**

Run:

```bash
bash -n scripts/backup-restore-drill.sh
```

Expected: FAIL because the script does not exist yet.

- [ ] **Step 2: Add the script skeleton and usage contract**

Create a Bash script with strict mode, dependency checks, and a small CLI that supports `--source-db`, `--restore-db-prefix`, and repeatable `--table` arguments.

```bash
#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/backup-restore-drill.sh [--source-db NAME] [--restore-db-prefix PREFIX] [--table TABLE]...

Defaults:
  --source-db observable
  --restore-db-prefix restore_drill
  --table tenants --table api_keys --table users --table user_sessions
EOF
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "FAIL: required command '$name' is not available" >&2
    exit 1
  fi
}

main() {
  echo "=== P4-S2 Backup And Restore Drill ==="
}

main "$@"
```

- [ ] **Step 3: Re-run the syntax check**

Run:

```bash
bash -n scripts/backup-restore-drill.sh
```

Expected: PASS.

- [ ] **Step 4: Commit the script skeleton**

Run:

```bash
git add scripts/backup-restore-drill.sh
git commit -m "Add backup restore drill skeleton"
```

Expected: commit succeeds on the feature branch.

---

### Task 2: Implement Logical Backup, Scratch Restore, And Validation

**Files:**
- Modify: `scripts/backup-restore-drill.sh`
- Test: `docker compose up -d postgres postgres-setup` followed by `bash scripts/backup-restore-drill.sh`

- [ ] **Step 1: Add the backup/restore implementation and row-validation helpers**

Expand the script so it:

1. Brings up `postgres` and `postgres-setup` if they are not already running.
2. Runs `pg_dump` from the `postgres` container against the source database.
3. Creates a scratch restore database named from the current UTC timestamp.
4. Restores the dump into the scratch database.
5. Compares source-versus-restore row counts for `tenants`, `api_keys`, `users`, and `user_sessions`.
6. Prints backup duration, restore duration, validation duration, and the scratch database name.
7. Drops the scratch database on exit, even on failure.

Use these helper functions in the implementation:

```bash
compose_exec_postgres() {
  docker compose exec -T postgres "$@"
}

db_count() {
  local db="$1" table="$2"
  compose_exec_postgres psql -U "$PG_USER" -d "$db" -Atc "SELECT COUNT(*) FROM \"$table\""
}

drop_restore_db() {
  compose_exec_postgres psql -U "$PG_USER" -d postgres -v ON_ERROR_STOP=1 \
    -c "DROP DATABASE IF EXISTS \"$RESTORE_DB\" WITH (FORCE);"
}
```

The restore path should use a plain SQL pipe so the drill does not require extra backup tooling:

```bash
compose_exec_postgres pg_dump -U "$PG_USER" -d "$SOURCE_DB" --no-owner --no-privileges \
  | compose_exec_postgres psql -U "$PG_USER" -d "$RESTORE_DB" -v ON_ERROR_STOP=1
```

- [ ] **Step 2: Run the drill against the seeded local database**

Run:

```bash
docker compose up -d postgres postgres-setup
bash scripts/backup-restore-drill.sh
```

Expected: PASS, with backup/restore timings printed and row-count validation succeeding for the representative table set.

- [ ] **Step 3: Re-run the syntax check and the drill**

Run:

```bash
bash -n scripts/backup-restore-drill.sh
bash scripts/backup-restore-drill.sh
```

Expected: PASS.

- [ ] **Step 4: Commit the implementation**

Run:

```bash
git add scripts/backup-restore-drill.sh
git commit -m "Add hot-store backup restore drill"
```

Expected: commit succeeds after the drill passes.

---

### Task 3: Update Agent Context And Mark The Roadmap Slice Complete

**Files:**
- Modify: `docs/agent-context.md`
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Test: `git diff --check`

- [ ] **Step 1: Update the active detailed plan pointer**

Set the active detailed implementation plan entry in `docs/agent-context.md` to:

```markdown
- Active detailed implementation plan: `docs/superpowers/plans/2026-05-22-p4-s2-backup-restore-drill.md` — P4-S2 hot-store restore drill for the shared PostgreSQL control-plane dataset.
```

- [ ] **Step 2: Mark P4-S2 complete in the roadmap**

After the drill passes, update `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` so P4-S2 reads:

```markdown
- [x] **P4-S2: Add backup and restore drill for one dataset**
  - Outcome: one restore path is practiced and timed.
  - Closure note: the first drill is hot-store-only because P4-S1 warm retention was deferred; object-storage state is therefore not part of this backup boundary yet.
  - Checkpoint: are measured RPO/RTO values acceptable? Answer: the local drill now prints the measured backup and restore durations for the seeded PostgreSQL control-plane dataset.
```

- [ ] **Step 3: Run documentation hygiene**

Run:

```bash
git diff --check
```

Expected: PASS.

- [ ] **Step 4: Commit the documentation updates**

Run:

```bash
git add docs/agent-context.md docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md
git commit -m "Document backup restore drill completion"
```

Expected: commit succeeds after the roadmap and agent-context entries match the implemented drill.

---

## Verification Plan

Required for the implementation PR:

```bash
bash -n scripts/backup-restore-drill.sh
docker compose up -d postgres postgres-setup
bash scripts/backup-restore-drill.sh
git diff --check
bash scripts/local-ci.sh
```

## ADR/Spec Synchronization

No ADR change is expected. The slice does not change the deployment model, storage architecture, security model, or data model; it operationalizes the existing restore-drill requirement already present in `spec/12-deployment.md` and `spec/11-testing.md`.
