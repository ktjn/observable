#!/bin/bash
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

SMOKE_COMPOSE_STARTED=0
REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="$REPO_ROOT/.local-ci"
TIMINGS="$LOG_DIR/timings.tsv"
LANE=""
CURRENT_STEP=""
STEP_STARTED=""
LANE_PIDS=()
LANE_NAMES=()
SUMMARY_READY=0

now() {
  if [[ -n "${EPOCHREALTIME:-}" ]]; then
    echo "${EPOCHREALTIME/,/.}"
  else
    date +%s
  fi
}

elapsed() { awk -v a="$1" -v b="$(now)" 'BEGIN { printf "%.1f", b - a }'; }

record_step() {
  [[ -n "$CURRENT_STEP" && $SUMMARY_READY -eq 1 ]] || return 0
  printf '%s\t%s\t%s\t%s\n' "${LANE:-main}" "$CURRENT_STEP" "$(elapsed "$STEP_STARTED")" "$1" >> "$TIMINGS"
}

say() {
  echo -e "$1"
  if [[ -n "$LANE" ]]; then
    echo -e "[$LANE] ${1#\\n}" >&3
  fi
}

step() { CURRENT_STEP=$1; STEP_STARTED=$(now); say "\n${BOLD}==> $1${NC}"; }
ok()   { record_step OK; say "${GREEN}OK${NC}  $1"; CURRENT_STEP=""; }
fail() { record_step FAIL; say "${RED}FAIL${NC} $1"; exit 1; }

stop_lanes() {
  local pid
  for pid in ${LANE_PIDS[@]+"${LANE_PIDS[@]}"}; do
    if kill -0 "$pid" 2>/dev/null; then
      kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
    fi
  done
  for pid in ${LANE_PIDS[@]+"${LANE_PIDS[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
  LANE_PIDS=()
}

print_summary() {
  [[ $SUMMARY_READY -eq 1 && -s "$TIMINGS" ]] || return 0
  echo -e "\n${BOLD}Stage timings${NC} (wall clock $(elapsed "$GATE_STARTED")s; logs in .local-ci/)"
  awk -F '\t' '{ printf "  %-9s %7ss  %-4s %s\n", $1, $3, $4, $2 }' "$TIMINGS"
}

cleanup_smoke_compose() {
  local status=$?

  stop_lanes

  if [[ $SMOKE_COMPOSE_STARTED -eq 1 ]]; then
    step "Compose cleanup"
    if docker compose --profile verification down --remove-orphans; then
      ok "docker compose down"
    else
      local cleanup_status=$?
      echo -e "${RED}FAIL${NC} docker compose down"
      if [[ $status -eq 0 ]]; then
        status=$cleanup_status
      fi
    fi
  fi

  print_summary
  trap - EXIT
  exit "$status"
}

trap cleanup_smoke_compose EXIT
cd "$REPO_ROOT"

SKIP_DOCKER=${SKIP_DOCKER:-0}
SKIP_FRONTEND=${SKIP_FRONTEND:-0}
SKIP_HELM=${SKIP_HELM:-0}
SKIP_MODELABLE=${SKIP_MODELABLE:-0}
SKIP_SMOKE=${SKIP_SMOKE:-0}
PARALLEL=${LOCAL_CI_PARALLEL:-1}
KEEP_GOING=${LOCAL_CI_KEEP_GOING:-0}

while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-docker)     SKIP_DOCKER=1     ;;
    --skip-frontend)   SKIP_FRONTEND=1   ;;
    --skip-helm)       SKIP_HELM=1       ;;
    --skip-modelable)  SKIP_MODELABLE=1  ;;
    --skip-smoke)      SKIP_SMOKE=1      ;;
    --serial)          PARALLEL=0        ;;
    --keep-going)      KEEP_GOING=1      ;;
    *) echo "Unknown flag: $1"; exit 1   ;;
  esac
  shift
done

mkdir -p "$LOG_DIR"
: > "$TIMINGS"
GATE_STARTED=$(now)
SUMMARY_READY=1

PYTHON_CMD="python"
if command -v python3 >/dev/null 2>&1; then
  PYTHON_CMD="python3"
fi

lane_checks() {
  step "CI policy regression tests"
  $PYTHON_CMD scripts/run-policy-tests.py && ok "CI policy regression tests" || fail "CI policy regression tests"

  step "Version metadata"
  bash scripts/verify-version.sh && ok "version metadata" || fail "version metadata"

  if [[ $SKIP_MODELABLE -eq 0 ]]; then
    check_modelable
  fi

  step "Distribution manifest"
  bash "$REPO_ROOT/scripts/check-distribution-manifest.sh" && ok "distribution manifest" || fail "distribution manifest"

  step "Component changelogs"
  bash "$REPO_ROOT/scripts/check-component-changelogs.sh" && ok "component changelogs" || fail "component changelogs"

  step "Pinned install artifact"
  bash "$REPO_ROOT/scripts/render-pinned-compose.sh" --check && ok "pinned install artifact" || fail "pinned install artifact"

  step "Component boundary lint"
  bash "$REPO_ROOT/scripts/check-component-boundaries.sh" && ok "component boundaries" || fail "component boundary lint"

  step "System specification consistency"
  python3 "$REPO_ROOT/scripts/tests/test_system_spec.py" && ok "system specification consistency" || fail "system specification consistency"

  step "System specification verification-stage validation"
  # 2.0 line: verification stage until the target release (docs/plans/business-spec-adoption.md Q7).
  # The stage runs separately because the CLI skips stage invariants when --check-artifacts/--check-drift pass.
  if command -v uv >/dev/null 2>&1; then
    { uv run --project "$REPO_ROOT/models" system-spec "$REPO_ROOT/spec/system.yaml" --stage verification \
      && uv run --project "$REPO_ROOT/models" system-spec "$REPO_ROOT/spec/system.yaml" --stage verification --check-artifacts --check-drift --root "$REPO_ROOT"; } \
      && ok "system specification verification validation" || fail "system specification verification validation"
    uv run --project "$REPO_ROOT/models" python "$REPO_ROOT/scripts/spec-readiness.py" --priority P0 \
      && ok "business spec readiness report" || fail "business spec readiness report"
  elif command -v system-spec >/dev/null 2>&1; then
    { system-spec "$REPO_ROOT/spec/system.yaml" --stage verification \
      && system-spec "$REPO_ROOT/spec/system.yaml" --stage verification --check-artifacts --check-drift --root "$REPO_ROOT"; } \
      && ok "system specification verification validation" || fail "system specification verification validation"
  fi

  step "System specification evidence provenance"
  python3 "$REPO_ROOT/scripts/update-spec-evidence.py" --check && ok "system spec evidence provenance" || fail "system spec evidence provenance"

  step "ClickHouse schema compatibility"
  bash "$REPO_ROOT/scripts/check-clickhouse-schema-compat.sh" && ok "clickhouse schema compatibility" || fail "clickhouse schema compatibility"

  step "ClickHouse migration immutability"
  bash "$REPO_ROOT/scripts/check-clickhouse-migration-immutability.sh" && ok "clickhouse migration immutability" || fail "clickhouse migration immutability"

  step "Formal verification (TLC)"
  bash "$REPO_ROOT/scripts/run-formal-verification.sh" && ok "formal verification" || fail "formal verification"

  if [[ $SKIP_HELM -eq 0 ]]; then
    step "Helm lint"
    if command -v helm >/dev/null 2>&1; then
      bash scripts/helm-lint.sh && ok "helm lint" || fail "helm lint"
    else
      fail "helm is required (run: bash scripts/helm-lint.sh)"
    fi
  fi
}

check_modelable() {
  if ! command -v uv >/dev/null 2>&1; then
    step "Modelable check"
    fail "uv is required (https://docs.astral.sh/uv/)"
  fi

  step "Modelable validate"
  uv run --project models modelable validate models/ && ok "modelable validate" || fail "modelable validate"

  step "Modelable regenerate diff-check"
  # WSL and Windows uv.exe have different /tmp namespaces. Keep temporary
  # outputs on the mounted repository so Bash and Windows tools see the same
  # files, and pass Windows uv the Windows spelling of those paths.
  TMP_TS="$(mktemp -d "$REPO_ROOT/.modelable-check.XXXXXX")"
  TMP_RS="$(mktemp -d "$REPO_ROOT/.modelable-check.XXXXXX")"
  TMP_SCHEMA="$(mktemp -d "$REPO_ROOT/.modelable-check.XXXXXX")"
  TMP_SCHEMA_TOOL="$TMP_SCHEMA"
  TMP_TS_TOOL="$TMP_TS"
  TMP_RS_TOOL="$TMP_RS"
  if uv --version 2>&1 | grep -q "windows-msvc" && command -v wslpath >/dev/null 2>&1; then
    TMP_SCHEMA_TOOL="$(wslpath -w "$TMP_SCHEMA")"
    TMP_TS_TOOL="$(wslpath -w "$TMP_TS")"
    TMP_RS_TOOL="$(wslpath -w "$TMP_RS")"
  fi
  TMP_RS_FILES="$TMP_RS/.rust-files"
  FAILED=0

  local ts_pid rs_pid schema_pid ts_status=0 rs_status=0 schema_status=0
  uv run --project models modelable compile models/ --target typescript --out "$TMP_TS_TOOL" >/dev/null 2>&1 & ts_pid=$!
  uv run --project models modelable compile models/ --target rust --out "$TMP_RS_TOOL" >/dev/null 2>&1 & rs_pid=$!
  uv run --project models modelable compile models/ --target json-schema --out "$TMP_SCHEMA_TOOL" >/dev/null 2>&1 & schema_pid=$!
  wait "$ts_pid" || ts_status=$?
  wait "$rs_pid" || rs_status=$?
  wait "$schema_pid" || schema_status=$?
  [[ $ts_status -eq 0 ]] || fail "modelable compile (typescript)"
  [[ $rs_status -eq 0 ]] || fail "modelable compile (rust)"
  [[ $schema_status -eq 0 ]] || fail "modelable compile (json-schema)"

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

  # Match the official regeneration path, which formats generated Rust via
  # workspace cargo fmt before committing it. Formatting the temporary output
  # preserves the byte-for-byte drift check while avoiding raw-emitter noise.
  find "$TMP_RS" -name '*.rs' -print0 > "$TMP_RS_FILES" || fail "enumerate generated Rust files"
  if [[ -s "$TMP_RS_FILES" ]]; then
    xargs -0 rustfmt --edition 2024 < "$TMP_RS_FILES" || fail "rustfmt temporary Modelable Rust"
  fi
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
  done < <(find libs/*/src/generated -mindepth 2 -name '*.rs' -print0)

  # JSON Schema — preserve the former GitHub generated-code coverage.
  while IFS= read -r -d '' f; do
    name="$(basename "$f")"
    if [[ ! -f "$TMP_SCHEMA/$name" ]] || ! diff -q "$f" "$TMP_SCHEMA/$name" >/dev/null 2>&1; then
      echo "  DRIFTED or MISSING in generated: $f"
      FAILED=1
    fi
  done < <(find contracts/schemas -mindepth 2 -name '*.json' -print0)

  rm -rf "$TMP_TS" "$TMP_RS" "$TMP_SCHEMA"

  if [ "$FAILED" -eq 1 ]; then
    echo ""
    echo "  Generated artifacts are out of sync with .mdl source files."
    echo "  Run:  bash scripts/regenerate-models.sh"
    echo "  Then review and commit the updated files."
    fail "modelable diff-check"
  fi
  ok "generated artifacts match .mdl files"

  step "Modelable breaking-change check"
  BREAKING_CHANGE_BASE_REF="$(git merge-base origin/main HEAD 2>/dev/null || git merge-base main HEAD 2>/dev/null || echo "")"
  if [[ -n "$BREAKING_CHANGE_BASE_REF" ]]; then
    bash "$REPO_ROOT/scripts/check-breaking-changes.sh" "$BREAKING_CHANGE_BASE_REF" HEAD \
      && ok "no unversioned breaking model/projection changes" \
      || fail "modelable breaking-change check"
  else
    fail "no local main/origin/main ref for model breaking-change check"
  fi
}

lane_rust() {
  step "Rust fmt"
  cargo fmt --all -- --check && ok "cargo fmt" || fail "cargo fmt"

  step "Rust clippy"
  cargo clippy --workspace --all-targets -- -D warnings && ok "cargo clippy" || fail "cargo clippy"

  step "Rust unit tests"
  cargo test --workspace --lib --bins && ok "cargo unit tests" || fail "cargo unit tests"

  if [[ $SKIP_DOCKER -eq 0 ]]; then
    step "Rust integration tests"
    cargo test --workspace --tests && ok "cargo integration tests" || fail "cargo integration tests"
  fi
}

lane_frontend() {
  step "Frontend typecheck"
  npm run typecheck --workspace=apps/frontend && ok "typecheck" || fail "typecheck"

  step "Frontend lint"
  npm run lint --workspace=apps/frontend && ok "lint" || fail "lint"

  step "Frontend build"
  npm run build --workspace=apps/frontend && ok "build" || fail "build"

  step "Frontend tests"
  npm run test --workspace=apps/frontend -- --run && ok "tests" || fail "tests"

  step "Frontend browser tests"
  CI=true npm run test:a11y --workspace=apps/frontend && ok "frontend browser tests" || fail "frontend browser tests (install Chromium with: npx playwright install chromium)"

  step "Playground build"
  bash scripts/build-playground.sh && ok "playground build" || fail "playground build"

  step "Playground browser smoke"
  (cd apps/frontend && CI=true PLAYGROUND_PREBUILT=1 npx playwright test --config=playwright.playground.config.ts) && ok "playground smoke" || fail "playground smoke"

  if [[ $SKIP_DOCKER -eq 0 ]]; then
    step "Frontend image build"
    docker compose build frontend && ok "frontend image build" || fail "frontend image build"
  fi
}

lane_images() {
  step "Shared image build (release/Helm)"
  if command -v docker-buildx >/dev/null 2>&1 || docker help buildx >/dev/null 2>&1; then
    docker buildx build --load --tag observable-services:local . && ok "docker build" || fail "docker build"
  else
    docker build --tag observable-services:local . && ok "docker build" || fail "docker build"
  fi

  step "Component image build (Compose)"
  docker compose --profile verification build \
    auth-service storage-writer stream-processor ingest-gateway query-api admin-service alert-evaluator \
    smoke-test && ok "component image build" || fail "component image build"

  step "Component image verification"
  bash scripts/check-component-images.sh && ok "component image verification" || fail "component image verification"
}

start_lane() {
  local name=$1
  shift
  if [[ $PARALLEL -eq 0 ]]; then
    "$@"
    return
  fi
  (
    trap - EXIT
    LANE=$name
    exec 3>&1 >"$LOG_DIR/$name.log" 2>&1 </dev/null
    "$@"
  ) &
  LANE_PIDS+=("$!")
  LANE_NAMES+=("$name")
  echo "[$name] started (log: .local-ci/$name.log)"
}

wait_lanes() {
  local failed=() pending=("${!LANE_PIDS[@]}") next i pid status
  while [[ ${#pending[@]} -gt 0 ]]; do
    next=()
    for i in "${pending[@]}"; do
      pid=${LANE_PIDS[$i]}
      if kill -0 "$pid" 2>/dev/null; then
        next+=("$i")
        continue
      fi
      status=0
      wait "$pid" || status=$?
      if [[ $status -ne 0 ]]; then
        failed+=("${LANE_NAMES[$i]}")
        echo -e "${RED}FAIL${NC} lane ${LANE_NAMES[$i]} (exit $status); last lines of .local-ci/${LANE_NAMES[$i]}.log:"
        tail -n 60 "$LOG_DIR/${LANE_NAMES[$i]}.log" | sed 's/^/    /'
      else
        echo -e "${GREEN}OK${NC}  lane ${LANE_NAMES[$i]}"
      fi
    done
    pending=(${next[@]+"${next[@]}"})
    if [[ ${#failed[@]} -gt 0 && $KEEP_GOING -eq 0 && ${#pending[@]} -gt 0 ]]; then
      echo -e "${YELLOW}Stopping remaining lanes (use --keep-going to finish them).${NC}"
      stop_lanes
      pending=()
    fi
    [[ ${#pending[@]} -eq 0 ]] || sleep 1
  done
  LANE_PIDS=()
  LANE_NAMES=()
  if [[ ${#failed[@]} -gt 0 ]]; then
    echo -e "${RED}FAIL${NC} lanes: ${failed[*]}"
    exit 1
  fi
}

[[ $PARALLEL -eq 0 ]] || set -m
start_lane checks lane_checks
start_lane rust lane_rust
if [[ $SKIP_FRONTEND -eq 0 ]]; then
  start_lane frontend lane_frontend
fi
if [[ $SKIP_DOCKER -eq 0 ]]; then
  start_lane images lane_images
fi
[[ $PARALLEL -eq 0 ]] || wait_lanes
set +m

if [[ $SKIP_DOCKER -eq 0 && $SKIP_SMOKE -eq 0 ]]; then
  step "Smoke test"
  SMOKE_COMPOSE_STARTED=1
  if docker compose up smoke-test --abort-on-container-exit; then
    ok "smoke-test"
    step "Performance smoke"
    docker compose up perf-smoke --abort-on-container-exit && ok "perf-smoke" || fail "perf-smoke"
  else
    docker compose logs --no-color --tail=120 ingest-gateway query-api stream-processor storage-writer || true
    fail "smoke-test"
  fi
fi

if [[ $SKIP_DOCKER -eq 1 || $SKIP_FRONTEND -eq 1 || $SKIP_HELM -eq 1 || $SKIP_MODELABLE -eq 1 || $SKIP_SMOKE -eq 1 ]]; then
  echo -e "\n${YELLOW}${BOLD}Selected checks passed; skipped stages are NOT validated.${NC}"
else
  echo -e "\n${GREEN}${BOLD}All checks passed.${NC}"
fi
