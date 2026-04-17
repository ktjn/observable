#!/usr/bin/env bash
set -euo pipefail
CH="clickhouse-client --host localhost --database observable"
for f in migrations/clickhouse/[0-9]*.sql; do
  echo "Applying $f..."
  $CH --multiquery < "$f"
done
echo "SELECT count() FROM spans" | $CH
echo "SELECT count() FROM logs" | $CH
echo "SELECT count() FROM metric_series" | $CH
echo "All migrations applied successfully."
