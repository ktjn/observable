#!/usr/bin/env bash
# Start all compiled services in the background.
# Usage: scripts/start-services.sh [build-dir]
# Sources .env.local if present; falls back to localhost defaults for CI.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${1:-$REPO_ROOT/target/release}"

# Load local overrides if present
if [ -f "$REPO_ROOT/.env.local" ]; then
  set -a; source "$REPO_ROOT/.env.local"; set +a
fi

# Defaults suitable for a fresh docker compose stack
export REDPANDA_BROKERS="${REDPANDA_BROKERS:-localhost:9092}"
export INGEST_TOPIC="${INGEST_TOPIC:-telemetry.raw}"
export DATABASE_URL="${DATABASE_URL:-postgres://observable:observable@localhost:5432/observable}"
export CLICKHOUSE_URL="${CLICKHOUSE_URL:-http://localhost:8123}"
export CLICKHOUSE_USER="${CLICKHOUSE_USER:-default}"
export CLICKHOUSE_PASSWORD="${CLICKHOUSE_PASSWORD:-observable}"
export AUTH_SERVICE_URL="${AUTH_SERVICE_URL:-http://localhost:4318}"

echo "Starting services from $BUILD_DIR ..."
"$BUILD_DIR/auth-service"        &
"$BUILD_DIR/storage-writer"      &
"$BUILD_DIR/stream-processor"    &
"$BUILD_DIR/ingest-gateway"      &
"$BUILD_DIR/query-api"           &

echo "Waiting for services to become ready..."
sleep 5
echo "Services started."
