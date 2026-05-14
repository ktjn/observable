#!/usr/bin/env bash
# reset-dev-volumes.sh — wipe local dev Docker volumes and restart with a clean state.
#
# When to use this:
#   - After a PostgreSQL major version bump in docker-compose.yml (e.g. PG16→PG17).
#     PostgreSQL data directories are not forward-compatible across major versions; the
#     container will crash with:
#       FATAL: database files are incompatible with server
#       DETAIL: The data directory was initialized by PostgreSQL version N.
#     Dropping the volume lets the new image re-initialize cleanly.  All schema is
#     captured in migrations/postgres/ and re-applied automatically by postgres-setup.
#
#   - After a ClickHouse or Redpanda major version bump that changes on-disk formats.
#   - Any time you want a completely clean local environment.
#
# Usage:
#   bash scripts/reset-dev-volumes.sh          # wipe postgres + shop_db only (most common)
#   bash scripts/reset-dev-volumes.sh --all    # wipe ALL observable volumes (full reset)

set -euo pipefail

PROJECT=observable
ALL=false

for arg in "$@"; do
  case $arg in
    --all) ALL=true ;;
    *) echo "Unknown argument: $arg"; exit 1 ;;
  esac
done

echo "==> Stopping all containers for project '${PROJECT}'..."
docker compose down

if [ "${ALL}" = "true" ]; then
  VOLUMES=(
    "${PROJECT}_postgres_data"
    "${PROJECT}_shop_db_data"
    "${PROJECT}_clickhouse_data"
    "${PROJECT}_redpanda_data"
    "${PROJECT}_zitadel-bootstrap"
  )
  echo "==> Removing ALL dev volumes..."
else
  VOLUMES=(
    "${PROJECT}_postgres_data"
    "${PROJECT}_shop_db_data"
  )
  echo "==> Removing PostgreSQL volumes (postgres_data, shop_db_data)..."
fi

for vol in "${VOLUMES[@]}"; do
  if docker volume inspect "${vol}" &>/dev/null; then
    docker volume rm "${vol}"
    echo "    removed: ${vol}"
  else
    echo "    skipped (not found): ${vol}"
  fi
done

echo ""
echo "Done. Run 'docker compose up --build' or 'make dev' to start fresh."
