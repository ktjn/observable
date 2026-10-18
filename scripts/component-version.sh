#!/usr/bin/env bash
set -euo pipefail

# Prints a component's version from the distribution manifest (Phase 7,
# docs/component-decomposition.md). The manifest is the authoritative source of
# per-component versions; see VERSIONING.md.
#
# Usage:
#   bash scripts/component-version.sh <component> [manifest]
#
# <component> is a target component name in kebab-case (as used by
# scripts/build-component-images.sh, e.g. store-clickhouse) or the manifest's
# camelCase key (e.g. storeClickhouse).

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <component> [manifest]" >&2
  exit 2
fi

COMPONENT="$1"
MANIFEST="${2:-$REPO_ROOT/distribution/manifest.yaml}"

if [[ ! -f "$MANIFEST" ]]; then
  echo "manifest not found: $MANIFEST" >&2
  exit 1
fi

IFS='-' read -ra parts <<<"$COMPONENT"
key="${parts[0]}"
for part in "${parts[@]:1}"; do
  key+="${part^}"
done

version="$(awk -v key="$key" '
  /^components:/ { f=1; next }
  f && /^[^[:space:]#]/ { exit }
  f && $1 == key":" { print $2; exit }
' "$MANIFEST")"

if [[ -z "$version" ]]; then
  echo "component '$COMPONENT' not found in $MANIFEST" >&2
  exit 1
fi

echo "$version"
