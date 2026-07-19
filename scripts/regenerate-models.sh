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
# Remaining known limitations (not patched here — suppressed at the module level):
#   Enum variants are emitted verbatim from .mdl (SCREAMING_SNAKE_CASE), which triggers
#   clippy::upper_case_acronyms. Suppressed via #![allow(clippy::upper_case_acronyms)]
#   in libs/domain/src/generated/tracing.rs.
#
# Fixed natively in 1.0.2:
#   #119 ClickHouse Row enum fields → String (with explicit match arms using raw wire values)
#   #123 TS imports placed before docblock → imports now follow meta block
#   #124 skip_serializing_if on clickhouse::Row → omitted natively for projections
#   #125 reverse From impls in domain files → From impls now only in projection files
#   #120 NamedType in same workspace resolved → use super:: imports, no EMIT003
#
# Fixed in 1.0.1 (no longer patched):
#   #118 TS NamedType imports → now auto-generated

echo ""
echo "==> Rust: cargo fmt generated files to match project style"
cargo fmt --all 2>/dev/null || true

echo ""
echo "==> All patches applied."
echo "==> Done. Run 'git diff --stat' to review changes."
