#!/bin/bash
set -euo pipefail

# Detect breaking changes to Modelable entities/projections whose version number
# did not change between BASE_REF and HEAD_REF. See check_breaking_changes.py for
# the exact rule. This is a compatibility check on top of `modelable compile`'s
# referential-integrity checks (scripts/check-generated-drift.sh), which does not
# by itself reject a self-consistent field removal/retype.
#
# Usage: bash scripts/check-breaking-changes.sh BASE_REF [HEAD_REF]
# HEAD_REF defaults to HEAD (the current working tree's committed state).

BASE_REF=${1:-}
HEAD_REF=${2:-HEAD}

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v uv >/dev/null 2>&1; then
  echo "SKIP: uv not installed, skipping breaking-change check."
  exit 0
fi

if [[ -z "$BASE_REF" ]] || ! git cat-file -e "${BASE_REF}^{commit}" 2>/dev/null; then
  echo "SKIP: no valid base ref to compare against, skipping breaking-change check."
  exit 0
fi

if git diff --quiet "$BASE_REF" "$HEAD_REF" -- models/ 2>/dev/null; then
  echo "No models/ changes between $BASE_REF and $HEAD_REF; skipping breaking-change check."
  exit 0
fi

BASE_TREE="$(mktemp -d)"
trap 'rm -rf "$BASE_TREE"' EXIT

if git cat-file -e "${BASE_REF}:models" 2>/dev/null; then
  git archive "$BASE_REF" -- models | tar -x -C "$BASE_TREE"
else
  echo "models/ did not exist at $BASE_REF; nothing to compare against."
  mkdir -p "$BASE_TREE/models"
fi

uv run --project models python "$REPO_ROOT/scripts/check_breaking_changes.py" \
  --base "$BASE_TREE/models" --head "$REPO_ROOT/models"
