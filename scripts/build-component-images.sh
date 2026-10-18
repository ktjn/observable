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
#   TAG       image tag (default: local). TAG=manifest tags each image with its
#             component version from distribution/manifest.yaml.
#   REGISTRY  optional registry/namespace prefix, e.g. ghcr.io/ktjn (default: none)
#   BUILD_TARGET Dockerfile target for Rust components (default: component-runtime).
#             CI uses component-runtime-ci to copy prebuilt binaries from bin/.

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TAG="${TAG:-local}"
REGISTRY="${REGISTRY:-}"
REGISTRY="${REGISTRY%/}"
BUILD_TARGET="${BUILD_TARGET:-component-runtime}"

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
  local component="$1" tag="$2"
  if [[ -n "$REGISTRY" ]]; then
    echo "${REGISTRY}/observable-${component}:${tag}"
  else
    echo "observable-${component}:${tag}"
  fi
}

for component in "${components[@]}"; do
  package="$(component_package "$component")"
  version="$(bash scripts/component-version.sh "$component")"
  if [[ "$TAG" == "manifest" ]]; then
    ref="$(image_ref "$component" "$version")"
  else
    ref="$(image_ref "$component" "$TAG")"
  fi
  label="org.opencontainers.image.version=${version}"

  echo "==> Building ${ref}"
  if [[ "$component" == "web" ]]; then
    docker buildx build --load --tag "$ref" --label "$label" -f apps/frontend/Dockerfile .
  else
    docker buildx build --load --target "$BUILD_TARGET" \
      --build-arg "SERVICE=${package}" --tag "$ref" --label "$label" .
  fi
  echo "OK  ${ref}"
done
