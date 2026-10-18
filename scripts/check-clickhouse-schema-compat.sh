#!/usr/bin/env bash
set -euo pipefail

# check-clickhouse-schema-compat.sh
#
# Validates the ClickHouse storage-schema compatibility contract between the
# store and query (Phase 3, docs/component-decomposition.md; ROADMAP 0.2:
# "declare a tested storage-schema compatibility range for observable-query").
#
# - observable-store-clickhouse declares its current schema version in
#   migrations/clickhouse/SCHEMA_VERSION. It must equal the highest-numbered
#   migration, so the declaration cannot drift from the applied schema.
# - observable-query declares the inclusive range of store schema versions it
#   has been tested against in services/query-api/clickhouse-schema-support.yaml.
# - The store's current version must fall inside query's range.
#
# Runnable locally (ADR-019); depends only on grep/awk/sed.
#
# Usage:
#   bash scripts/check-clickhouse-schema-compat.sh [repo-root]

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_ROOT="${1:-$REPO_ROOT}"

MIGRATIONS_DIR="$SOURCE_ROOT/migrations/clickhouse"
STORE_VERSION_FILE="$MIGRATIONS_DIR/SCHEMA_VERSION"
QUERY_SUPPORT_FILE="$SOURCE_ROOT/services/query-api/clickhouse-schema-support.yaml"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

rel() { echo "${1#"$SOURCE_ROOT"/}"; }

[[ -f "$STORE_VERSION_FILE" ]] \
  || fail "store schema version file not found: $(rel "$STORE_VERSION_FILE")"
store_version="$(tr -d '[:space:]' < "$STORE_VERSION_FILE")"
[[ "$store_version" =~ ^[0-9]+$ ]] \
  || fail "SCHEMA_VERSION '$store_version' is not a non-negative integer"

latest=0
found_migration=0
for f in "$MIGRATIONS_DIR"/[0-9]*.sql; do
  [[ -e "$f" ]] || break
  found_migration=1
  name="$(basename "$f")"
  number="${name%%_*}"
  [[ "$number" =~ ^[0-9]+$ ]] || fail "migration '$name' does not start with a number"
  value=$((10#$number))
  if (( value > latest )); then
    latest=$value
  fi
done
[[ $found_migration -eq 1 ]] || fail "no ClickHouse migrations found in $(rel "$MIGRATIONS_DIR")"
[[ "$store_version" -eq "$latest" ]] \
  || fail "SCHEMA_VERSION '$store_version' does not match the highest ClickHouse migration '$latest'"

[[ -f "$QUERY_SUPPORT_FILE" ]] \
  || fail "query schema-support file not found: $(rel "$QUERY_SUPPORT_FILE")"
read_field() {
  sed -n -E "s/^$1:[[:space:]]*//p" "$QUERY_SUPPORT_FILE" | head -1
}
min="$(read_field min)"
max="$(read_field max)"
[[ "$min" =~ ^[0-9]+$ ]] || fail "query schema-support min '$min' is not a non-negative integer"
[[ "$max" =~ ^[0-9]+$ ]] || fail "query schema-support max '$max' is not a non-negative integer"
(( min <= max )) || fail "query schema-support range is empty: min $min > max $max"

if (( store_version < min || store_version > max )); then
  fail "store ClickHouse schema version $store_version is outside observable-query's tested range [$min, $max]; re-test query against the new schema and update $(rel "$QUERY_SUPPORT_FILE")"
fi

echo "OK  ClickHouse schema $store_version within observable-query's tested range [$min, $max]"
