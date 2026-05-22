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

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

require_command docker
require_command date
require_command awk
require_command tr

SOURCE_DB="observable"
RESTORE_DB_PREFIX="restore_drill"
TABLES=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-db)
      SOURCE_DB="${2:-}"
      shift 2
      ;;
    --restore-db-prefix)
      RESTORE_DB_PREFIX="${2:-}"
      shift 2
      ;;
    --table)
      TABLES+=("${2:-}")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "FAIL: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$SOURCE_DB" || -z "$RESTORE_DB_PREFIX" ]]; then
  echo "FAIL: source database and restore prefix must not be empty" >&2
  exit 1
fi

if [[ "${#TABLES[@]}" -eq 0 ]]; then
  TABLES=(tenants api_keys users user_sessions)
fi

if [[ "${#TABLES[@]}" -eq 0 ]]; then
  echo "FAIL: at least one table must be selected" >&2
  exit 1
fi

PG_USER="${PG_USER:-observable}"
PG_PASSWORD="${PG_PASSWORD:-observable}"

compose_exec_postgres() {
  docker compose exec -T -e "PGPASSWORD=$PG_PASSWORD" postgres "$@"
}

elapsed_ms() {
  local start_ns="$1" end_ns="$2"
  awk -v start="$start_ns" -v end="$end_ns" 'BEGIN { printf "%.3f", (end - start) / 1000000 }'
}

db_count() {
  local db="$1" table="$2"
  compose_exec_postgres psql -U "$PG_USER" -d "$db" -Atc "SELECT COUNT(*) FROM \"$table\"" | tr -d '[:space:]'
}

drop_restore_db() {
  if [[ -n "${RESTORE_DB:-}" ]]; then
    compose_exec_postgres psql -U "$PG_USER" -d postgres -v ON_ERROR_STOP=1 \
      -c "DROP DATABASE IF EXISTS \"$RESTORE_DB\" WITH (FORCE);" >/dev/null
  fi
}

trap drop_restore_db EXIT

echo "=== P4-S2 Backup And Restore Drill ==="
echo "Source database   : $SOURCE_DB"
echo "Restore prefix    : $RESTORE_DB_PREFIX"
echo "Tables validated  : ${TABLES[*]}"

docker compose up -d postgres postgres-setup >/dev/null

source_check_start_ns="$(date +%s%N)"
compose_exec_postgres psql -U "$PG_USER" -d "$SOURCE_DB" -Atc "SELECT 1" >/dev/null
source_check_end_ns="$(date +%s%N)"
echo "Source check (ms) : $(elapsed_ms "$source_check_start_ns" "$source_check_end_ns")"

source_counts=()
for table in "${TABLES[@]}"; do
  source_counts+=("$(db_count "$SOURCE_DB" "$table")")
done

timestamp="$(date -u +%Y%m%dT%H%M%SZ | tr -d ':')"
RESTORE_DB="${RESTORE_DB_PREFIX}_${timestamp}"

drop_restore_db
compose_exec_postgres createdb -U "$PG_USER" "$RESTORE_DB" >/dev/null

backup_start_ns="$(date +%s%N)"
compose_exec_postgres pg_dump -U "$PG_USER" -d "$SOURCE_DB" --no-owner --no-privileges \
  | compose_exec_postgres psql -U "$PG_USER" -d "$RESTORE_DB" -v ON_ERROR_STOP=1 >/dev/null
backup_end_ns="$(date +%s%N)"

validate_start_ns="$(date +%s%N)"
restore_counts=()
for table in "${TABLES[@]}"; do
  restore_counts+=("$(db_count "$RESTORE_DB" "$table")")
done

for i in "${!TABLES[@]}"; do
  if [[ "${source_counts[$i]}" != "${restore_counts[$i]}" ]]; then
    echo "FAIL: row-count mismatch for table '${TABLES[$i]}' (source=${source_counts[$i]}, restore=${restore_counts[$i]})" >&2
    exit 1
  fi
done
validate_end_ns="$(date +%s%N)"

echo "Backup duration   : $(elapsed_ms "$backup_start_ns" "$backup_end_ns") ms"
echo "Restore database  : $RESTORE_DB"
echo "Validation ms     : $(elapsed_ms "$validate_start_ns" "$validate_end_ns")"
echo "Row counts        : match for ${#TABLES[@]} table(s)"
echo "Status            : PASS"
