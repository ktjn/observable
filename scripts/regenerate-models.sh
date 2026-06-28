#!/usr/bin/env bash
# Regenerate all modelable-generated artifacts from .mdl source files.
#
# Usage:
#   bash scripts/regenerate-models.sh
#
# Run this from the repo root after changing any .mdl file in models/.
# Commits the regenerated artifacts — review the diff before pushing.
#
# Prerequisites: uv with Python >=3.14 on PATH.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

TMP_TS="$(mktemp -d)"
TMP_RS="$(mktemp -d)"
trap 'rm -rf "$TMP_TS" "$TMP_RS"' EXIT

echo "==> Compiling TypeScript artifacts"
uv run --project models modelable compile models/ --target typescript --out "$TMP_TS"
echo ""

echo "==> Copying TypeScript files to apps/frontend/src/api/generated/"
for f in "$TMP_TS"/*.ts; do
  name="$(basename "$f")"
  domain="${name%%.*}"
  mkdir -p "apps/frontend/src/api/generated/$domain"
  cp "$f" "apps/frontend/src/api/generated/$domain/$name"
done

echo "==> Copying pipeline TypeScript files to crypto-aggregator demos"
for f in "$TMP_TS"/pipeline.*.ts; do
  mkdir -p "demos/crypto-aggregator/backend/src/generated"
  mkdir -p "demos/crypto-aggregator/frontend/src/generated"
  cp "$f" "demos/crypto-aggregator/backend/src/generated/"
  cp "$f" "demos/crypto-aggregator/frontend/src/generated/"
done
echo ""

echo "==> Compiling Rust artifacts"
uv run --project models modelable compile models/ --target rust --out "$TMP_RS"
echo ""

echo "==> Copying Rust files to libs/domain/src/generated/"
for domain_dir in "$TMP_RS"/*/; do
  domain="$(basename "$domain_dir")"
  if [ -d "libs/domain/src/generated/$domain" ]; then
    cp "$domain_dir"/*.rs "libs/domain/src/generated/$domain/"
    echo "  copied $domain/*.rs"
  fi
done
echo ""

#
# Post-processing patches for modelable emitter limitations
# See: https://github.com/ktjn/modelable/issues/118 (TS imports)
#      https://github.com/ktjn/modelable/issues/119 (Rust From impls)
#      https://github.com/ktjn/modelable/issues/120 (Rust NamedType warning)
#

echo ""
echo "==> Patching: add missing TS import statements for NamedType references"

# nlq.NlqIr.v0.ts — NlqIr references NlqFilter and NlqTimeRange
NLQ_IR="apps/frontend/src/api/generated/nlq/nlq.NlqIr.v0.ts"
sed -i '/^ \*\/$/a\
import type { NlqFilter } from "./nlq.NlqFilter.v0";\
import type { NlqTimeRange } from "./nlq.NlqTimeRange.v0";' "$NLQ_IR"

# dashboards.Dashboard.v1.ts — Dashboard references DashboardPanel
DASHBOARD_V1="apps/frontend/src/api/generated/dashboards/dashboards.Dashboard.v1.ts"
sed -i '/^ \*\/$/a\
import type { DashboardPanel } from "./dashboards.DashboardPanel.v0";' "$DASHBOARD_V1"

# dashboards.DashboardPanel.v0.ts — DashboardPanel references DashboardPanelLayout
DASHBOARD_PANEL="apps/frontend/src/api/generated/dashboards/dashboards.DashboardPanel.v0.ts"
sed -i '/^ \*\/$/a\
import type { DashboardPanelLayout } from "./dashboards.DashboardPanelLayout.v0";' "$DASHBOARD_PANEL"

echo ""
echo "==> Patching: fix TracingSpanRowV1 enum fields to String for ClickHouse compatibility"

# tracing_span_row_v1.rs — modelable v1.0.0 generates typed enums for span_kind/status_code,
# but clickhouse-rs 0.15 panics when serializing unit-variant enums for String columns.
# Patch: replace typed enum fields with String, update From<TracingSpanV1> to produce
# SCREAMING_SNAKE_CASE strings, and update the file header comment.
# See: https://github.com/ktjn/modelable/issues/119
SPAN_ROW_RS="libs/domain/src/generated/tracing/tracing_span_row_v1.rs"
if [ -f "$SPAN_ROW_RS" ]; then
  # Replace typed enum field declarations with String in the struct
  sed -i \
    -e 's/pub span_kind: TracingSpanRowV1SpanKind,/pub span_kind: String,/' \
    -e 's/pub status_code: TracingSpanRowV1StatusCode,/pub status_code: String,/' \
    "$SPAN_ROW_RS"

  # Replace .into() enum conversions in From<TracingSpanV1> with explicit match-to-string
  python3 - "$SPAN_ROW_RS" << 'PYEOF'
import re, sys

path = sys.argv[1]
src = open(path).read()

src = src.replace(
    "span_kind: src.span_kind.into(),",
    """span_kind: match src.span_kind {
                TracingSpanV1SpanKind::Internal => "INTERNAL".to_string(),
                TracingSpanV1SpanKind::Server => "SERVER".to_string(),
                TracingSpanV1SpanKind::Client => "CLIENT".to_string(),
                TracingSpanV1SpanKind::Producer => "PRODUCER".to_string(),
                TracingSpanV1SpanKind::Consumer => "CONSUMER".to_string(),
            },""",
)
src = src.replace(
    "status_code: src.status_code.into(),",
    """status_code: match src.status_code {
                TracingSpanV1StatusCode::Unset => "UNSET".to_string(),
                TracingSpanV1StatusCode::Ok => "OK".to_string(),
                TracingSpanV1StatusCode::Error => "ERROR".to_string(),
            },""",
)

open(path, "w").write(src)
PYEOF
fi

echo ""
echo "==> Rust: cargo fmt generated files to match project style"
cargo fmt --all 2>/dev/null || true

echo ""
echo "==> All patches applied."
echo "==> Done. Run 'git diff --stat' to review changes."
