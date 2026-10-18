#!/bin/bash
set -euo pipefail

BASE_REF=${1:-}
HEAD_REF=${2:-}
OUTPUT_FILE=${GITHUB_OUTPUT:-}

backend=0
frontend=0
helm=0
smoke=0
models=0
distribution=0

mark_all() {
  backend=1
  frontend=1
  helm=1
  smoke=1
  models=1
  distribution=1
}

write_output() {
  local key=$1
  local value=$2

  if [[ -n "$OUTPUT_FILE" ]]; then
    echo "${key}=${value}" >> "$OUTPUT_FILE"
  else
    echo "${key}=${value}"
  fi
}

if [[ -z "$BASE_REF" || -z "$HEAD_REF" || "$BASE_REF" =~ ^0+$ ]]; then
  mark_all
  models=1
  components="$(python3 scripts/ci_changed_components.py --all)"
else
  while IFS= read -r path; do
    case "$path" in
      .github/workflows/build.yml|scripts/ci-changed-surfaces.sh|scripts/ci_changed_components.py)
        mark_all
        ;;
      Cargo.toml|Cargo.lock|Dockerfile|.dockerignore|docker-compose*.yml|proto/*|proto/**|services/*|services/**|libs/*|libs/**|migrations/*|migrations/**|tests/*|tests/**)
        backend=1
        smoke=1
        ;;
      scripts/local-ci.sh|scripts/ci.sh|scripts/migrate.sh|scripts/start-services.sh|tests/e2e/smoke_test.sh)
        backend=1
        smoke=1
        ;;
      package.json|package-lock.json|apps/frontend/*|apps/frontend/**)
        frontend=1
        ;;
      charts/*|charts/**|scripts/helm-lint.sh)
        helm=1
        ;;
      models/*|models/**|libs/*/src/generated/*|libs/*/src/generated/**|apps/frontend/src/api/generated/*|apps/frontend/src/api/generated/**)
        models=1
        ;;
      distribution/*|distribution/**|scripts/check-distribution-manifest.sh|scripts/build-component-images.sh|scripts/component-version.sh)
        distribution=1
        ;;
    esac
  done < <(git diff --name-only "$BASE_REF" "$HEAD_REF")
  components="$(python3 scripts/ci_changed_components.py "$BASE_REF" "$HEAD_REF")"
fi

# Rust components whose release binary the diff affects, for service-scoped
# artifact builds (Phase 7, docs/component-decomposition.md).
rust_components="$(tr ' ' '\n' <<<"$components" | grep -vx 'web' | paste -sd' ' - || true)"

write_output backend "$backend"
write_output frontend "$frontend"
write_output helm "$helm"
write_output smoke "$smoke"
write_output models "$models"
write_output distribution "$distribution"
write_output components "$components"
write_output rust_components "$rust_components"
