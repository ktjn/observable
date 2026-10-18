#!/usr/bin/env bash
set -euo pipefail

# check-clickhouse-migration-immutability.sh
#
# Applied migrations must be immutable. The store applies every
# migrations/clickhouse/[0-9]*.sql on startup (CREATE TABLE IF NOT EXISTS), so
# editing a migration that already shipped silently changes the schema a fresh
# deployment gets and diverges it from deployments that already migrated. Add a
# new numbered migration instead.
#
# migrations/clickhouse/MIGRATION_CHECKSUMS records a sha256 per committed
# migration. This gate fails when a recorded migration is edited or removed, or
# when a migration is present but not recorded.
#
# Usage:
#   bash scripts/check-clickhouse-migration-immutability.sh           # verify
#   bash scripts/check-clickhouse-migration-immutability.sh --update   # record

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MIGRATIONS_DIR="$REPO_ROOT/migrations/clickhouse"
CHECKSUMS="$MIGRATIONS_DIR/MIGRATION_CHECKSUMS"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    fail "no sha256 tool (sha256sum/shasum) on PATH"
  fi
}

compute() {
  local f
  for f in "$MIGRATIONS_DIR"/[0-9]*.sql; do
    [[ -e "$f" ]] || fail "no ClickHouse migrations found in migrations/clickhouse"
    printf '%s  %s\n' "$(sha256_of "$f")" "$(basename "$f")"
  done
}

if [[ "${1:-}" == "--update" ]]; then
  compute > "$CHECKSUMS"
  echo "OK  recorded $(awk 'NF' "$CHECKSUMS" | wc -l) ClickHouse migration checksum(s)"
  exit 0
fi

[[ -f "$CHECKSUMS" ]] \
  || fail "migrations/clickhouse/MIGRATION_CHECKSUMS is missing; run scripts/check-clickhouse-migration-immutability.sh --update"

# Every recorded migration must still be present and byte-identical.
while read -r sum name; do
  [[ -z "$name" ]] && continue
  path="$MIGRATIONS_DIR/$name"
  [[ -f "$path" ]] || fail "recorded migration '$name' is missing; applied migrations must not be removed"
  current="$(sha256_of "$path")"
  [[ "$current" == "$sum" ]] \
    || fail "migration '$name' was edited; applied migrations are immutable (add a new numbered migration instead)"
done < "$CHECKSUMS"

# Every migration must be recorded.
tmp="$(mktemp)"
compute > "$tmp"
while read -r _ name; do
  [[ -z "$name" ]] && continue
  awk -v n="$name" '$2 == n { found = 1 } END { exit !found }' "$CHECKSUMS" \
    || { rm -f "$tmp"; fail "migration '$name' is not recorded; run scripts/check-clickhouse-migration-immutability.sh --update"; }
done < "$tmp"
rm -f "$tmp"

echo "OK  ClickHouse migrations are immutable and fully recorded"
