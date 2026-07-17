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
SKIP_HELM=${SKIP_HELM:-0}
SKIP_MODELABLE=${SKIP_MODELABLE:-0}
SKIP_SMOKE=${SKIP_SMOKE:-0}

while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-docker)     SKIP_DOCKER=1     ;;
    --skip-frontend)   SKIP_FRONTEND=1   ;;
    --skip-helm)       SKIP_HELM=1       ;;
    --skip-modelable)  SKIP_MODELABLE=1  ;;
    --skip-smoke)      SKIP_SMOKE=1      ;;
    *) echo "Unknown flag: $1"; exit 1   ;;
  esac
  shift
done

if [[ $SKIP_MODELABLE -eq 0 ]]; then
  if command -v uv >/dev/null 2>&1; then
    step "Modelable validate"
    uv run --project models modelable validate models/ && ok "modelable validate" || fail "modelable validate"

    step "Modelable regenerate diff-check"
    TMP_TS="$(mktemp -d)"
    TMP_RS="$(mktemp -d)"
    TMP_RS_FILES="$TMP_RS/.rust-files"
    FAILED=0

    uv run --project models modelable compile models/ --target typescript --out "$TMP_TS" >/dev/null 2>&1 || fail "modelable compile (typescript)"
    uv run --project models modelable compile models/ --target rust --out "$TMP_RS" >/dev/null 2>&1 || fail "modelable compile (rust)"

    # TypeScript — main frontend
    while IFS= read -r -d '' f; do
      name="$(basename "$f")"
      if [ ! -f "$TMP_TS/$name" ]; then
        echo "  MISSING in generated: $f"
        FAILED=1
        continue
      fi
      if ! diff -q "$f" "$TMP_TS/$name" >/dev/null 2>&1; then
        echo "  DRIFTED: $f"
        FAILED=1
      fi
    done < <(find apps/frontend/src/api/generated -name '*.ts' -print0)

    # TypeScript — crypto-aggregator demos
    while IFS= read -r -d '' f; do
      name="$(basename "$f")"
      if [ ! -f "$TMP_TS/$name" ]; then
        echo "  MISSING in generated: $f"
        FAILED=1
        continue
      fi
      if ! diff -q "$f" "$TMP_TS/$name" >/dev/null 2>&1; then
        echo "  DRIFTED: $f"
        FAILED=1
      fi
    done < <(find demos/crypto-aggregator -path '*/generated/*.ts' -print0)

    # Match the official regeneration path, which formats generated Rust via
    # workspace cargo fmt before committing it. Formatting the temporary output
    # preserves the byte-for-byte drift check while avoiding raw-emitter noise.
    find "$TMP_RS" -name '*.rs' -print0 > "$TMP_RS_FILES" || fail "enumerate generated Rust files"
    while IFS= read -r -d '' f; do
      rustfmt --edition 2024 "$f" || fail "rustfmt temporary Modelable Rust: $f"
    done < "$TMP_RS_FILES"
    rm -f "$TMP_RS_FILES"

    # Rust — only subdirectory files, not hand-maintained module files
    while IFS= read -r -d '' f; do
      name="$(basename "$f")"
      domain="$(basename "$(dirname "$f")")"
      if [ ! -f "$TMP_RS/$domain/$name" ]; then
        echo "  MISSING in generated: $f"
        FAILED=1
        continue
      fi
      if ! diff -q "$f" "$TMP_RS/$domain/$name" >/dev/null 2>&1; then
        echo "  DRIFTED: $f"
        FAILED=1
      fi
    done < <(find libs/domain/src/generated -mindepth 2 -name '*.rs' -print0)

    rm -rf "$TMP_TS" "$TMP_RS"

    if [ "$FAILED" -eq 1 ]; then
      echo ""
      echo "  Generated artifacts are out of sync with .mdl source files."
      echo "  Run:  bash scripts/regenerate-models.sh"
      echo "  Then review and commit the updated files."
      fail "modelable diff-check"
    fi
    ok "generated artifacts match .mdl files"
  else
    step "Modelable check"
    echo "SKIP  modelable (uv not installed — see https://docs.astral.sh/uv/)"
  fi
fi

step "Rust fmt"
cargo fmt --all -- --check && ok "cargo fmt" || fail "cargo fmt"

step "Rust clippy"
cargo clippy --workspace --all-targets -- -D warnings && ok "cargo clippy" || fail "cargo clippy"

step "Rust unit tests"
cargo test --workspace --lib --bins && ok "cargo unit tests" || fail "cargo unit tests"

if [[ $SKIP_FRONTEND -eq 0 ]]; then
  step "Frontend typecheck"
  npm run typecheck --workspace=apps/frontend && ok "typecheck" || fail "typecheck"

  step "Frontend lint"
  npm run lint --workspace=apps/frontend && ok "lint" || fail "lint"

  step "Frontend build"
  npm run build --workspace=apps/frontend && ok "build" || fail "build"

  step "Frontend tests"
  npm run test --workspace=apps/frontend -- --run && ok "tests" || fail "tests"

  step "Frontend accessibility tests"
  if node -e "
    const { chromium } = require('./apps/frontend/node_modules/playwright-core');
    const fs = require('fs');
    process.exit(fs.existsSync(chromium.executablePath()) ? 0 : 1);
  " 2>/dev/null; then
    npm run test:a11y --workspace=apps/frontend && ok "a11y" || fail "a11y"
  else
    echo "SKIP  a11y (Chromium not installed — run: cd apps/frontend && npx playwright install chromium)"
  fi
fi

if [[ $SKIP_HELM -eq 0 ]]; then
  step "Helm lint"
  if command -v helm >/dev/null 2>&1; then
    bash scripts/helm-lint.sh && ok "helm lint" || fail "helm lint"
  else
    echo "SKIP  helm lint (helm not installed — run: bash scripts/helm-lint.sh)"
  fi
fi

if [[ $SKIP_DOCKER -eq 0 ]]; then
  step "Rust integration tests"
  cargo test --workspace --tests && ok "cargo integration tests" || fail "cargo integration tests"

  step "Docker image build"
  if command -v docker-buildx >/dev/null 2>&1 || docker help buildx >/dev/null 2>&1; then
    docker buildx build --load --tag observable-services:local . && ok "docker build" || fail "docker build"
  else
    docker build --tag observable-services:local . && ok "docker build" || fail "docker build"
  fi

  if [[ $SKIP_FRONTEND -eq 0 ]]; then
    step "Frontend image build"
    docker compose build frontend && ok "frontend image build" || fail "frontend image build"
  fi

  if [[ $SKIP_SMOKE -eq 0 ]]; then
    step "Smoke test"
    SMOKE_COMPOSE_STARTED=1
    if docker compose up smoke-test --abort-on-container-exit; then
      ok "smoke-test"
    else
      docker compose logs --no-color --tail=120 ingest-gateway query-api stream-processor storage-writer || true
      fail "smoke-test"
    fi
  fi
fi

echo -e "\n${GREEN}${BOLD}All checks passed.${NC}"
