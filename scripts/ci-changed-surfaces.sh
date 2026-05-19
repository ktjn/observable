#!/bin/bash
set -euo pipefail

BASE_REF=${1:-}
HEAD_REF=${2:-}
OUTPUT_FILE=${GITHUB_OUTPUT:-}

backend=0
frontend=0
helm=0
smoke=0

mark_all() {
  backend=1
  frontend=1
  helm=1
  smoke=1
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
else
  while IFS= read -r path; do
    case "$path" in
      .github/workflows/build.yml|scripts/ci-changed-surfaces.sh)
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
    esac
  done < <(git diff --name-only "$BASE_REF" "$HEAD_REF")
fi

write_output backend "$backend"
write_output frontend "$frontend"
write_output helm "$helm"
write_output smoke "$smoke"
