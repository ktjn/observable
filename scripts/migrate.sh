#!/usr/bin/env bash
# Run all database migrations against the running docker compose stack.
# Requires: docker compose up -d (stack must be healthy before calling this).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "=== ClickHouse migrations ==="
docker compose -f "$REPO_ROOT/docker-compose.yml" exec -T clickhouse \
  clickhouse-client --query "CREATE DATABASE IF NOT EXISTS observable"
for f in "$REPO_ROOT"/migrations/clickhouse/*.sql; do
  echo "  applying $(basename "$f")"
  docker compose -f "$REPO_ROOT/docker-compose.yml" exec -T clickhouse \
    clickhouse-client --multiquery < "$f"
done

echo "=== Redpanda topic ==="
docker compose -f "$REPO_ROOT/docker-compose.yml" exec -T redpanda \
  rpk topic create telemetry.raw --partitions 3 --replicas 1 2>/dev/null || true

echo "=== PostgreSQL migrations ==="
for f in "$REPO_ROOT"/migrations/postgres/*.sql; do
  echo "  applying $(basename "$f")"
  cat "$f" | docker compose -f "$REPO_ROOT/docker-compose.yml" exec -T postgres \
    psql -U observable -d observable -f -
done

echo "=== Migrations complete ==="
