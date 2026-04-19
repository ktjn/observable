#!/bin/bash
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

step() { echo -e "\n${BOLD}==> $1${NC}"; }
ok()   { echo -e "${GREEN}OK${NC}  $1"; }
fail() { echo -e "${RED}FAIL${NC} $1"; exit 1; }

SMOKE_COMPOSE_STARTED=0

cleanup_smoke_compose() {
  local status=$?

  if [[ $SMOKE_COMPOSE_STARTED -eq 1 ]]; then
    step "Compose cleanup"
    if docker compose down --remove-orphans; then
      ok "docker compose down"
    else
      local cleanup_status=$?
      echo -e "${RED}FAIL${NC} docker compose down"
      if [[ $status -eq 0 ]]; then
        status=$cleanup_status
      fi
    fi
  fi

  trap - EXIT
  exit "$status"
}

trap cleanup_smoke_compose EXIT

SKIP_DOCKER=${SKIP_DOCKER:-0}
SKIP_FRONTEND=${SKIP_FRONTEND:-0}
SKIP_SMOKE=${SKIP_SMOKE:-0}

while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-docker)   SKIP_DOCKER=1   ;;
    --skip-frontend) SKIP_FRONTEND=1 ;;
    --skip-smoke)    SKIP_SMOKE=1    ;;
    *) echo "Unknown flag: $1"; exit 1 ;;
  esac
  shift
done

step "Rust fmt"
cargo fmt --all -- --check && ok "cargo fmt" || fail "cargo fmt"

step "Rust clippy"
cargo clippy --workspace --all-targets -- -D warnings && ok "cargo clippy" || fail "cargo clippy"

step "Rust tests"
cargo test --workspace --all-targets && ok "cargo test" || fail "cargo test"

if [[ $SKIP_FRONTEND -eq 0 ]]; then
  step "Frontend typecheck"
  npm run typecheck --workspace=apps/frontend && ok "typecheck" || fail "typecheck"

  step "Frontend lint"
  npm run lint --workspace=apps/frontend && ok "lint" || fail "lint"

  step "Frontend build"
  npm run build --workspace=apps/frontend && ok "build" || fail "build"

  step "Frontend tests"
  npm run test --workspace=apps/frontend -- --run && ok "tests" || fail "tests"
fi

if [[ $SKIP_DOCKER -eq 0 ]]; then
  step "Docker image build"
  if command -v docker-buildx >/dev/null 2>&1 || docker help buildx >/dev/null 2>&1; then
    docker buildx build --load --tag observable-services:local . && ok "docker build" || fail "docker build"
  else
    docker build --tag observable-services:local . && ok "docker build" || fail "docker build"
  fi

  if [[ $SKIP_SMOKE -eq 0 ]]; then
    step "Smoke test"
    SMOKE_COMPOSE_STARTED=1
    docker compose up smoke-test --abort-on-container-exit && ok "smoke-test" || fail "smoke-test"
  fi
fi

echo -e "\n${GREEN}${BOLD}All checks passed.${NC}"
