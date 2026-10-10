#!/usr/bin/env bash
set -euo pipefail

# Validates the distribution manifest (Phase 7, docs/component-decomposition.md):
# a productVersion and a version for every target component, all valid SemVer,
# with no missing or unexpected component keys. Runnable locally and in CI
# (ADR-019); depends only on grep/awk/sed.

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${1:-$REPO_ROOT/distribution/manifest.yaml}"

if [[ ! -f "$MANIFEST" ]]; then
  echo "FAIL: manifest not found: $MANIFEST" >&2
  exit 1
fi

SEMVER='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
EXPECTED=(contracts auth control ingest process storeClickhouse query alerting web)

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

is_semver() {
  [[ "$1" =~ $SEMVER ]]
}

product_version="$(grep -E '^productVersion:' "$MANIFEST" | head -1 | sed -E 's/^productVersion:[[:space:]]*//')"
[[ -n "$product_version" ]] || fail "missing productVersion"
is_semver "$product_version" || fail "productVersion '$product_version' is not valid SemVer"

declare -A versions=()
while IFS= read -r line; do
  key="$(sed -E 's/^[[:space:]]+//; s/:.*//' <<<"$line")"
  value="$(sed -E 's/^[^:]+:[[:space:]]*//' <<<"$line")"
  versions["$key"]="$value"
done < <(awk '/^components:/{f=1; next} f && /^  [A-Za-z]/{print}' "$MANIFEST")

[[ ${#versions[@]} -gt 0 ]] || fail "no components found under 'components:'"

for component in "${EXPECTED[@]}"; do
  value="${versions[$component]:-}"
  [[ -n "$value" ]] || fail "component '$component' missing from manifest"
  is_semver "$value" || fail "component '$component' version '$value' is not valid SemVer"
done

for key in "${!versions[@]}"; do
  found=0
  for component in "${EXPECTED[@]}"; do
    [[ "$key" == "$component" ]] && found=1
  done
  [[ $found -eq 1 ]] || fail "unexpected component '$key' in manifest"
done

echo "OK  distribution manifest valid (productVersion ${product_version}, ${#EXPECTED[@]} components)"
