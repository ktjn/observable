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
TMP_SCHEMA="$(mktemp -d)"
REGISTRY_IDS="$REPO_ROOT/registry-ids.lock"
trap 'rm -rf "$TMP_TS" "$TMP_RS" "$TMP_SCHEMA"' EXIT

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

echo "==> Copying Rust files to their checked-in generated/ directories"
# Each modelable domain's output directory is copied to whichever crate already
# has a matching libs/*/src/generated/<domain> directory checked in. tracing and
# logs live under libs/observable-storage-contracts (ClickHouse row projections);
# other domains may add a libs/domain/src/generated/<domain> directory later.
for domain_dir in "$TMP_RS"/*/; do
  domain="$(basename "$domain_dir")"
  copied=0
  for crate_generated in libs/*/src/generated; do
    if [ -d "$crate_generated/$domain" ]; then
      cp "$domain_dir"/*.rs "$crate_generated/$domain/"
      echo "  copied $domain/*.rs -> $crate_generated/$domain/"
      copied=1
    fi
  done
  if [ "$copied" -eq 0 ]; then
    echo "  skipped $domain (no checked-in generated/$domain directory)"
  fi
done
echo ""

echo "==> Compiling JSON Schema artifacts"
uv run --project models modelable compile models/ --target json-schema --out "$TMP_SCHEMA" --registry-ids "$REGISTRY_IDS"
echo ""

echo "==> Copying JSON Schema files to their checked-in contracts/schemas/ directories"
# json-schema output is flat (domain.Type.vN.json, no per-domain subdirectory), unlike
# the rust/typescript targets. Only copy files whose domain already has a checked-in
# contracts/schemas/<domain>/ directory; this is the wire-schema counterpart to the
# telemetry.raw.v1 event contract, not a general schema-publishing pipeline.
for f in "$TMP_SCHEMA"/*.json; do
  name="$(basename "$f")"
  domain="${name%%.*}"
  if [ -d "contracts/schemas/$domain" ] && [ -f "contracts/schemas/$domain/$name" ]; then
    cp "$f" "contracts/schemas/$domain/$name"
    echo "  copied $name -> contracts/schemas/$domain/"
  fi
done
echo ""

echo ""
echo "==> Rust: cargo fmt generated files to match project style"
cargo fmt --all 2>/dev/null || true

echo ""
echo "==> Generation complete."
echo "==> Done. Run 'git diff --stat' to review changes."
