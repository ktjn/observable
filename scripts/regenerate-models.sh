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
REGISTRY_IDS="$REPO_ROOT/registry-ids.lock"
trap 'rm -rf "$TMP_TS" "$TMP_RS"' EXIT

echo "==> Compiling TypeScript artifacts"
uv run --project models modelable compile models/ --target typescript --out "$TMP_TS" --registry-ids "$REGISTRY_IDS"
echo ""

echo "==> Copying TypeScript files to apps/frontend/src/api/generated/"
for f in "$TMP_TS"/*.ts; do
  name="$(basename "$f")"
  domain="${name%%.*}"
  mkdir -p "apps/frontend/src/api/generated/$domain"
  cp "$f" "apps/frontend/src/api/generated/$domain/$name"
done


echo "==> Compiling Rust artifacts"
uv run --project models modelable compile models/ --target rust --out "$TMP_RS" --registry-ids "$REGISTRY_IDS"
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

echo ""
echo "==> Rust: cargo fmt generated files to match project style"
cargo fmt --all 2>/dev/null || true

echo ""
echo "==> Generation complete."
echo "==> Done. Run 'git diff --stat' to review changes."
