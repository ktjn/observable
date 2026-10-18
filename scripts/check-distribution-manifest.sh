#!/usr/bin/env bash
set -euo pipefail

# Validates the distribution manifest (Phase 7, docs/component-decomposition.md):
# a productVersion matching the root VERSION and a version for every target
# component, all valid SemVer, with no missing or unexpected component keys.
# Each component's own package version (Cargo package or frontend package.json)
# must equal its manifest version, so a component version changes only by
# editing both together. The Helm chart's per-component image pins
# (charts/observable/values.yaml services.<key>.image) must name the component
# image and carry the same version. Runnable locally (ADR-019); depends only on
# grep/awk/sed.
#
# Usage:
#   bash scripts/check-distribution-manifest.sh [manifest] [repo-root]

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${1:-$REPO_ROOT/distribution/manifest.yaml}"
SOURCE_ROOT="${2:-$REPO_ROOT}"

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

# manifest key -> file that carries the component's own package version
# (empty for components without a single owning package yet).
component_source() {
  case "$1" in
    auth) echo "services/auth-service/Cargo.toml" ;;
    control) echo "services/admin-service/Cargo.toml" ;;
    ingest) echo "services/ingest-gateway/Cargo.toml" ;;
    process) echo "services/stream-processor/Cargo.toml" ;;
    storeClickhouse) echo "services/storage-writer/Cargo.toml" ;;
    query) echo "services/query-api/Cargo.toml" ;;
    alerting) echo "services/alert-evaluator/Cargo.toml" ;;
    web) echo "apps/frontend/package.json" ;;
    contracts) echo "" ;;
  esac
}

cargo_package_version() {
  awk '
    /^\[package\]$/ { in_package=1; next }
    /^\[/ { if (in_package) exit }
    in_package && /^version[[:space:]]*=/ {
      value=$0
      sub(/^[^=]*=[[:space:]]*"/, "", value)
      sub(/".*$/, "", value)
      print value
      exit
    }
  ' "$1"
}

# manifest key -> Helm services.<key> and image repository.
component_helm_key() {
  case "$1" in
    auth) echo "authService observable-auth" ;;
    control) echo "adminService observable-control" ;;
    ingest) echo "ingestGateway observable-ingest" ;;
    process) echo "streamProcessor observable-process" ;;
    storeClickhouse) echo "storageWriter observable-store-clickhouse" ;;
    query) echo "queryApi observable-query" ;;
    alerting) echo "alertEvaluator observable-alerting" ;;
    web) echo "frontend observable-web" ;;
    contracts) echo "" ;;
  esac
}

# Prints "repository tag" for services.<key>.image in a Helm values file.
helm_component_image() {
  awk -v key="$2" '
    /^services:/ { in_services=1; next }
    in_services && /^[^[:space:]#]/ { exit }
    in_services && /^  [A-Za-z]/ { current=$1; sub(/:$/, "", current); next }
    in_services && current == key && /^    image:/ {
      line=$0
      repo=line; sub(/.*repository:[[:space:]]*/, "", repo); sub(/[,[:space:]}].*/, "", repo)
      tag=line; sub(/.*tag:[[:space:]]*"?/, "", tag); sub(/["[:space:],}].*/, "", tag)
      print repo, tag
      exit
    }
  ' "$1"
}

package_json_version() {
  sed -n -E 's/^  "version":[[:space:]]*"([^"]+)".*/\1/p' "$1" | head -1
}

product_version="$(grep -E '^productVersion:' "$MANIFEST" | head -1 | sed -E 's/^productVersion:[[:space:]]*//')"
[[ -n "$product_version" ]] || fail "missing productVersion"
is_semver "$product_version" || fail "productVersion '$product_version' is not valid SemVer"

root_version="$(tr -d '[:space:]' < "$SOURCE_ROOT/VERSION")"
[[ "$product_version" == "$root_version" ]] \
  || fail "productVersion '$product_version' does not match VERSION '$root_version'"

declare -A versions=()
while IFS= read -r line; do
  key="$(sed -E 's/^[[:space:]]+//; s/:.*//' <<<"$line")"
  value="$(sed -E 's/^[^:]+:[[:space:]]*//' <<<"$line")"
  versions["$key"]="$value"
done < <(awk '/^components:/{f=1; next} f && /^[^[:space:]#]/{exit} f && /^  [A-Za-z]/{print}' "$MANIFEST")

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

for component in "${EXPECTED[@]}"; do
  source="$(component_source "$component")"
  [[ -n "$source" ]] || continue
  path="$SOURCE_ROOT/$source"
  [[ -f "$path" ]] || fail "component '$component' source not found: $source"
  if [[ "$source" == *.json ]]; then
    actual="$(package_json_version "$path")"
  else
    actual="$(cargo_package_version "$path")"
  fi
  [[ "$actual" == "${versions[$component]}" ]] \
    || fail "$source version '$actual' does not match manifest component '$component' version '${versions[$component]}'"
done

HELM_VALUES="$SOURCE_ROOT/charts/observable/values.yaml"
[[ -f "$HELM_VALUES" ]] || fail "Helm values not found: charts/observable/values.yaml"
for component in "${EXPECTED[@]}"; do
  mapping="$(component_helm_key "$component")"
  [[ -n "$mapping" ]] || continue
  read -r helm_key expected_repo <<<"$mapping"
  read -r repo tag <<<"$(helm_component_image "$HELM_VALUES" "$helm_key")"
  [[ "$repo" == "$expected_repo" ]] \
    || fail "Helm services.$helm_key.image.repository '$repo' does not match '$expected_repo'"
  [[ "$tag" == "${versions[$component]}" ]] \
    || fail "Helm services.$helm_key.image.tag '$tag' does not match manifest component '$component' version '${versions[$component]}'"
done

echo "OK  distribution manifest valid (productVersion ${product_version}, ${#EXPECTED[@]} components)"
