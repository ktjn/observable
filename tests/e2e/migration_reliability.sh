#!/usr/bin/env bash
set -euo pipefail

# Migration reliability test: verifies idempotency and restart resilience.
# Part of Milestone 3 of the ROADMAP.md.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$REPO_ROOT"

log() { echo "--- [$(date +%H:%M:%S)] $*"; }

cleanup() {
  log "Cleaning up..."
  docker compose down -v >/dev/null 2>&1 || true
}

trap cleanup EXIT

log "Starting clean infrastructure..."
docker compose up -d clickhouse postgres redpanda
# Wait for healthy
log "Waiting for services to be healthy..."
for i in {1..30}; do
  if docker compose ps | grep -q "healthy"; then
    # This is a bit weak, let's check specifically
    if docker compose exec -T postgres pg_isready -U observable >/dev/null 2>&1; then
      break
    fi
  fi
  sleep 2
done

log "Phase 1: Running migrations first time..."
docker compose run --rm clickhouse-setup
docker compose run --rm postgres-setup
docker compose run --rm redpanda-setup

log "Phase 2: Running migrations second time (idempotency check)..."
# Should succeed without errors even if tables/columns already exist.
docker compose run --rm clickhouse-setup
docker compose run --rm postgres-setup
docker compose run --rm redpanda-setup

log "Phase 3: Verifying final state..."
# Verify a few key tables exist in PostgreSQL
docker compose exec -T postgres psql -U observable -d observable -c "\dt" | grep -q "tenants"
docker compose exec -T postgres psql -U observable -d observable -c "\dt" | grep -q "api_keys"
docker compose exec -T postgres psql -U observable -d observable -c "\dt" | grep -q "dashboards"

# Verify tables exist in ClickHouse
docker compose exec -T clickhouse clickhouse-client --query "EXISTS TABLE observable.spans" | grep -q "1"
docker compose exec -T clickhouse clickhouse-client --query "EXISTS TABLE observable.logs" | grep -q "1"
docker compose exec -T clickhouse clickhouse-client --query "EXISTS TABLE observable.metrics" | grep -q "1"

log "MIGRATION RELIABILITY TEST PASSED"
