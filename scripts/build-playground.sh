#!/bin/bash
set -euo pipefail

# Builds the experimental GitHub Pages WASM playground artifact:
#   1. compile libs/playground-wasm to wasm32-unknown-unknown via wasm-pack
#   2. install frontend deps and build the playground Vite variant
#   3. verify no accidental root-relative asset URLs leaked into the build
#
# See docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md.
# This script does not deploy anything; it only produces a static artifact
# under apps/frontend/dist for local verification.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Checking for wasm-pack"
if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack not found on PATH. Install it with: cargo install wasm-pack"
  exit 1
fi

echo "==> Ensuring wasm32-unknown-unknown target is installed"
rustup target add wasm32-unknown-unknown

echo "==> Building libs/playground-wasm for the browser"
wasm-pack build libs/playground-wasm \
  --target web \
  --out-dir "$REPO_ROOT/apps/frontend/public/playground/wasm"

echo "==> Installing frontend dependencies"
npm ci

echo "==> Building the playground frontend variant"
npm run build:playground --workspace=apps/frontend

echo "==> Verifying no root-relative asset paths leaked into the build"
DIST="apps/frontend/dist"
if grep -RIl --include="*.html" --include="*.js" --include="*.css" -E '(href|src)="/(?!observable/)' "$DIST" >/dev/null 2>&1; then
  echo "Found asset URLs rooted outside the /observable/ base path:"
  grep -RIln --include="*.html" --include="*.js" --include="*.css" -E '(href|src)="/(?!observable/)' "$DIST"
  exit 1
fi

echo "==> Playground artifact ready at $DIST"
