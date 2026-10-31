#!/usr/bin/env bash
set -euo pipefail

# check-clickhouse-schema-compat.sh
#
# Validates the ClickHouse storage-schema compatibility contract between the
# store (observable-store-clickhouse) and direct readers (observable-query and
# observable-alerting):
#
# - observable-store-clickhouse declares its current schema version in
#   migrations/clickhouse/SCHEMA_VERSION. It must equal the highest-numbered
#   migration, so the declaration cannot drift from the applied schema.
# - observable-query declares its tested schema range in
#   services/query-api/clickhouse-schema-support.yaml.
# - observable-alerting declares its tested schema range in
#   services/alert-evaluator/clickhouse-schema-support.yaml.
# - The store's current version must fall inside both readers' ranges.
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
ALERTING_SUPPORT_FILE="$SOURCE_ROOT/services/alert-evaluator/clickhouse-schema-support.yaml"

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

check_reader_range() {
  local label="$1"
  local component="$2"
  local support_file="$3"

  [[ -f "$support_file" ]] \
    || fail "$label schema-support file not found: $(rel "$support_file")"
  local min max
  min="$(sed -n -E "s/^min:[[:space:]]*//p" "$support_file" | tr -d '\r' | head -1)"
  max="$(sed -n -E "s/^max:[[:space:]]*//p" "$support_file" | tr -d '\r' | head -1)"
  [[ "$min" =~ ^[0-9]+$ ]] || fail "$label schema-support min '$min' is not a non-negative integer"
  [[ "$max" =~ ^[0-9]+$ ]] || fail "$label schema-support max '$max' is not a non-negative integer"
  (( min <= max )) || fail "$label schema-support range is empty: min $min > max $max"

  if (( store_version < min || store_version > max )); then
    fail "store ClickHouse schema version $store_version is outside $component's tested range [$min, $max]; re-test $label against the new schema and update $(rel "$support_file")"
  fi

  echo "OK  ClickHouse schema $store_version within $component's tested range [$min, $max]"
}

check_reader_range "query" "observable-query" "$QUERY_SUPPORT_FILE"
check_reader_range "alerting" "observable-alerting" "$ALERTING_SUPPORT_FILE"
