#!/usr/bin/env bash
set -euo pipefail

# Synthetic pre-0.1 upgrade test.
# Verifies that applying current migrations on top of a partially-migrated state
# (simulating an older version) preserves data and succeeds.
# Part of Milestone 3 of the ROADMAP.md.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$REPO_ROOT"

log() { echo "--- [$(date +%H:%M:%S)] $*"; }

TEMP_MIGRATIONS="$REPO_ROOT/temp-upgrade-migrations"

cleanup() {
  log "Cleaning up..."
  rm -rf "$TEMP_MIGRATIONS"
  docker compose down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT

log "Starting clean infrastructure..."
docker compose up -d postgres
# Wait for healthy
for i in {1..30}; do
  if docker compose exec -T postgres pg_isready -U observable >/dev/null 2>&1; then
    break
  fi
  sleep 2
done

log "Phase 1: Preparing 'old' version (subset of migrations)..."
mkdir -p "$TEMP_MIGRATIONS/postgres"
# We take the first 5 migrations as our "old" version
cp "$REPO_ROOT/migrations/postgres/001_create_tenants.sql" "$TEMP_MIGRATIONS/postgres/"
cp "$REPO_ROOT/migrations/postgres/002_create_api_keys.sql" "$TEMP_MIGRATIONS/postgres/"
cp "$REPO_ROOT/migrations/postgres/003_create_projects.sql" "$TEMP_MIGRATIONS/postgres/"
cp "$REPO_ROOT/migrations/postgres/004_create_credential_audit_log.sql" "$TEMP_MIGRATIONS/postgres/"
cp "$REPO_ROOT/migrations/postgres/005_create_query_audit_log.sql" "$TEMP_MIGRATIONS/postgres/"

log "Applying 'old' migrations..."
# We use a temporary container to apply these so we don't change the main setup service
docker run --rm \
  --network observable_default \
  -v "$TEMP_MIGRATIONS/postgres:/migrations:ro" \
  -e PGPASSWORD=observable \
  postgres:18 \
  /bin/bash -c "for f in /migrations/*.sql; do psql -U observable -d observable -h postgres -f \"\$f\"; done"

log "Phase 2: Inserting legacy data into 'old' schema..."
docker compose exec -T postgres psql -U observable -d observable -c \
  "INSERT INTO tenants (id, name) VALUES ('11111111-1111-1111-1111-111111111111', 'legacy-tenant') ON CONFLICT DO NOTHING;"

log "Phase 3: Running upgrade (applying ALL current migrations)..."
docker compose run --rm postgres-setup

log "Phase 4: Verifying data after upgrade..."
# Verify legacy data survived
docker compose exec -T postgres psql -U observable -d observable -c \
  "SELECT name FROM tenants WHERE id = '11111111-1111-1111-1111-111111111111';" | grep -q "legacy-tenant"

# Verify a "new" table from a later migration exists (e.g. dashboards from 012)
docker compose exec -T postgres psql -U observable -d observable -c "\dt" | grep -q "dashboards"

log "UPGRADE TEST PASSED"
