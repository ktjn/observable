#!/bin/bash
set -euo pipefail

# Verify that checked-in generated code matches what modelable produces
# from the .mdl source files. Exits non-zero if any file has drifted.

TMP_TS="$(mktemp -d)"
TMP_RS="$(mktemp -d)"
TMP_RS_FILES="$TMP_RS/.rust-files"
FAILED=0

trap 'rm -rf "$TMP_TS" "$TMP_RS"' EXIT

if ! command -v uv >/dev/null 2>&1; then
  echo "SKIP: uv not installed, skipping drift check."
  exit 0
fi

echo "Compiling .mdl files..."
uv run --project models modelable compile models/ --target typescript --out "$TMP_TS"
uv run --project models modelable compile models/ --target rust --out "$TMP_RS"

# TypeScript — main frontend
while IFS= read -r -d '' f; do
  name="$(basename "$f")"
  if [ ! -f "$TMP_TS/$name" ]; then
    echo "MISSING in generated: $f"
    FAILED=1
    continue
  fi
  if ! diff -q "$f" "$TMP_TS/$name" >/dev/null 2>&1; then
    echo "DRIFTED: $f"
    FAILED=1
  fi
done < <(find apps/frontend/src/api/generated -name '*.ts' -print0)

# TypeScript — crypto-aggregator demos
while IFS= read -r -d '' f; do
  name="$(basename "$f")"
  if [ ! -f "$TMP_TS/$name" ]; then
    echo "MISSING in generated: $f"
    FAILED=1
    continue
  fi
  if ! diff -q "$f" "$TMP_TS/$name" >/dev/null 2>&1; then
    echo "DRIFTED: $f"
    FAILED=1
  fi
done < <(find demos/crypto-aggregator -path '*/generated/*.ts' -print0)

# Format generated Rust to match the regeneration script's cargo fmt step
find "$TMP_RS" -name '*.rs' -print0 > "$TMP_RS_FILES"
while IFS= read -r -d '' f; do
  rustfmt --edition 2024 "$f"
done < "$TMP_RS_FILES"
rm -f "$TMP_RS_FILES"

# Rust — only subdirectory files, not hand-maintained module files
while IFS= read -r -d '' f; do
  name="$(basename "$f")"
  domain="$(basename "$(dirname "$f")")"
  if [ ! -f "$TMP_RS/$domain/$name" ]; then
    echo "MISSING in generated: $f"
    FAILED=1
    continue
  fi
  if ! diff -q "$f" "$TMP_RS/$domain/$name" >/dev/null 2>&1; then
    echo "DRIFTED: $f"
    FAILED=1
  fi
done < <(find libs/domain/src/generated -mindepth 2 -name '*.rs' -print0)

if [ "$FAILED" -eq 1 ]; then
  echo ""
  echo "Generated artifacts are out of sync with .mdl source files."
  echo "Run:  bash scripts/regenerate-models.sh"
  echo "Then review and commit the updated files."
  exit 1
fi

echo "All generated artifacts match .mdl files."
