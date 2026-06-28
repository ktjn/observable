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
echo "==> Patching: add From<&str> impls for generated Rust enums"

# tracing_span_row_v1.rs — add From<&str> for span_kind and status_code enums
# (modelable v1.0.0 uses typed enums; many call sites use .into() from &str)
SPAN_ROW_RS="libs/domain/src/generated/tracing/tracing_span_row_v1.rs"
if [ -f "$SPAN_ROW_RS" ]; then
  # Remove any previous manual From<&str> impls (marked by comment marker)
  sed -i '/^# -- From<&str> impls for enum backward compat --$/,/^# -- end From<&str> impls --$/d' "$SPAN_ROW_RS"
  cat >> "$SPAN_ROW_RS" << 'RSPATCH'
# -- From<&str> impls for enum backward compat --
impl From<&str> for TracingSpanRowV1SpanKind {
    fn from(src: &str) -> Self {
        match src.to_uppercase().as_str() {
            "INTERNAL" => Self::Internal,
            "SERVER" => Self::Server,
            "CLIENT" => Self::Client,
            "PRODUCER" => Self::Producer,
            "CONSUMER" => Self::Consumer,
            _ => Self::Internal,
        }
    }
}
impl From<&str> for TracingSpanRowV1StatusCode {
    fn from(src: &str) -> Self {
        match src.to_uppercase().as_str() {
            "UNSET" => Self::Unset,
            "OK" => Self::Ok,
            "ERROR" => Self::Error,
            _ => Self::Unset,
        }
    }
}
# -- end From<&str> impls --
RSPATCH
fi

echo ""
echo "==> Rust: cargo fmt generated files to match project style"
cargo fmt --all 2>/dev/null || true

echo ""
echo "==> All patches applied."
echo "==> Done. Run 'git diff --stat' to review changes."
