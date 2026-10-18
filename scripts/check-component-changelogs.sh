#!/usr/bin/env bash
set -euo pipefail

# check-component-changelogs.sh
#
# Validates that every component in the distribution manifest has a
# per-component CHANGELOG.md carrying an entry for its current version
# (docs/component-decomposition.md §Release Model; ROADMAP 0.2 exit evidence
# "every component can release independently with SemVer, changelog, SBOM, and
# provenance"). A component whose version changes must add a changelog entry in
# the same change.
#
# Runnable locally (ADR-019); depends only on grep/awk/sed.
#
# Usage:
#   bash scripts/check-component-changelogs.sh [manifest] [repo-root]

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${1:-$REPO_ROOT/distribution/manifest.yaml}"
SOURCE_ROOT="${2:-$REPO_ROOT}"

EXPECTED=(contracts auth control ingest process storeClickhouse query alerting web)

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# manifest key -> per-component changelog path (the component that moves to its
# own repository during extraction keeps its changelog).
component_changelog() {
  case "$1" in
    contracts) echo "contracts/CHANGELOG.md" ;;
    auth) echo "services/auth-service/CHANGELOG.md" ;;
    control) echo "services/admin-service/CHANGELOG.md" ;;
    ingest) echo "services/ingest-gateway/CHANGELOG.md" ;;
    process) echo "services/stream-processor/CHANGELOG.md" ;;
    storeClickhouse) echo "services/storage-writer/CHANGELOG.md" ;;
    query) echo "services/query-api/CHANGELOG.md" ;;
    alerting) echo "services/alert-evaluator/CHANGELOG.md" ;;
    web) echo "apps/frontend/CHANGELOG.md" ;;
  esac
}

[[ -f "$MANIFEST" ]] || fail "manifest not found: $MANIFEST"

declare -A versions=()
while IFS= read -r line; do
  key="$(sed -E 's/^[[:space:]]+//; s/:.*//' <<<"$line")"
  value="$(sed -E 's/^[^:]+:[[:space:]]*//' <<<"$line")"
  versions["$key"]="$value"
done < <(awk '/^components:/{f=1; next} f && /^[^[:space:]#]/{exit} f && /^  [A-Za-z]/{print}' "$MANIFEST")

for component in "${EXPECTED[@]}"; do
  version="${versions[$component]:-}"
  [[ -n "$version" ]] || fail "component '$component' missing from $MANIFEST"
  rel="$(component_changelog "$component")"
  path="$SOURCE_ROOT/$rel"
  [[ -f "$path" ]] || fail "missing changelog for component '$component': $rel"
  grep -qF "## [$version]" "$path" \
    || fail "$rel has no changelog entry for version $version (add '## [$version] - <date>')"
done

echo "OK  component changelogs present for ${#EXPECTED[@]} components"
