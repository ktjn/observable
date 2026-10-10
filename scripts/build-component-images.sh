#!/usr/bin/env bash
set -euo pipefail

# Builds one container image per deployable component (Phase 7,
# docs/component-decomposition.md). The Dockerfile's `component-runtime` target
# builds only the requested Cargo package, so a change to one component does not
# rebuild another component's binary.
#
# Usage:
#   bash scripts/build-component-images.sh [component ...]
#
# With no arguments, builds every component. Component names are the target
# component names from docs/component-decomposition.md. `web` builds the
# frontend image from apps/frontend/Dockerfile; the rest map to a Cargo package.
#
# Environment:
#   TAG       image tag (default: local)
#   REGISTRY  optional registry/namespace prefix, e.g. ghcr.io/ktjn (default: none)

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TAG="${TAG:-local}"
REGISTRY="${REGISTRY:-}"
REGISTRY="${REGISTRY%/}"

# component name -> Cargo package (empty for components with a separate build).
component_package() {
  case "$1" in
    auth) echo "auth-service" ;;
    control) echo "admin-service" ;;
    ingest) echo "ingest-gateway" ;;
    process) echo "stream-processor" ;;
    store-clickhouse) echo "storage-writer" ;;
    query) echo "query-api" ;;
    alerting) echo "alert-evaluator" ;;
    web) echo "" ;;
    *) echo "unknown component: $1" >&2; return 1 ;;
  esac
}

ALL_COMPONENTS=(auth control ingest process store-clickhouse query alerting web)

if [[ $# -gt 0 ]]; then
  components=("$@")
else
  components=("${ALL_COMPONENTS[@]}")
fi

image_ref() {
  local component="$1"
  if [[ -n "$REGISTRY" ]]; then
    echo "${REGISTRY}/observable-${component}:${TAG}"
  else
    echo "observable-${component}:${TAG}"
  fi
}

for component in "${components[@]}"; do
  package="$(component_package "$component")"
  ref="$(image_ref "$component")"

  echo "==> Building ${ref}"
  if [[ "$component" == "web" ]]; then
    docker buildx build --load --tag "$ref" -f apps/frontend/Dockerfile .
  else
    docker buildx build --load --target component-runtime \
      --build-arg "SERVICE=${package}" --tag "$ref" .
  fi
  echo "OK  ${ref}"
done
