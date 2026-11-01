#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUITE_SCRIPT="$SCRIPT_DIR/../../scripts/release-candidate-suites.sh"

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "FAIL: $message"
    echo "expected: $expected"
    echo "actual:   $actual"
    exit 1
  fi
}

run_test() {
  local name="$1"
  shift

  echo "==> $name"
  "$@"
}

test_exports_main_without_running_flow() {
  local output
  output="$(
    RELEASE_CANDIDATE_SUITES_SOURCE_ONLY=1 bash -c '
      source "$1"
      declare -F main >/dev/null
      echo loaded
    ' bash "$SUITE_SCRIPT" 2>&1
  )"

  assert_eq "loaded" "$output" "release-candidate-suites.sh should load helpers without executing the suite"
}

test_suite_runs_stages_in_order() {
  local output
  output="$(
    RELEASE_CANDIDATE_SUITES_SOURCE_ONLY=1 bash -c '
      source "$1"
      docker() {
        printf "%s\n" "docker $*"
      }
      bash() {
        printf "%s\n" "bash $*"
      }
      kind() { :; }
      kubectl() { :; }
      helm() { :; }
      main
    ' bash "$SUITE_SCRIPT" 2>&1
  )"

  if [[ "$output" != *"docker compose up -d --wait auth-service storage-writer stream-processor ingest-gateway query-api alert-evaluator frontend"* ]]; then
    echo "FAIL: suite should bootstrap the core compose stack first"
    echo "$output"
    exit 1
  fi

  if [[ "$output" != *"docker compose run --rm perf-smoke"* ]]; then
    echo "FAIL: suite should run the perf smoke stage"
    echo "$output"
    exit 1
  fi

  if [[ "$output" != *"docker compose run --rm smoke-test"* ]]; then
    echo "FAIL: suite should run the tenant-escape smoke stage"
    echo "$output"
    exit 1
  fi

  if [[ "$output" != *"chaos-smoke.sh"* ]]; then
    echo "FAIL: suite should run the chaos probe"
    echo "$output"
    exit 1
  fi

  if [[ "$output" != *"kind-test.sh"* ]]; then
    echo "FAIL: suite should run the kind rollback gate"
    echo "$output"
    exit 1
  fi
}

test_suite_with_doctor_runs_doctor_stages() {
  local output
  output="$(
    RELEASE_CANDIDATE_SUITES_SOURCE_ONLY=1 bash -c '
      source "$1"
      docker() { printf "%s\n" "docker $*"; }
      bash() { printf "%s\n" "bash $*"; }
      kind() { :; }
      kubectl() { :; }
      helm() { :; }
      main --with-doctor
    ' bash "$SUITE_SCRIPT" 2>&1
  )"

  if [[ "$output" != *"bash "*"/doctor.sh --preflight"* ]]; then
    echo "FAIL: suite with --with-doctor should run doctor preflight"
    echo "$output"
    exit 1
  fi

  if [[ "$output" != *"bash "*"/doctor.sh"* ]]; then
    echo "FAIL: suite with --with-doctor should run doctor diagnostics"
    echo "$output"
    exit 1
  fi
}

test_suite_with_pinned_runs_pinned_install() {
  local output
  output="$(
    RELEASE_CANDIDATE_SUITES_SOURCE_ONLY=1 bash -c '
      source "$1"
      docker() { printf "%s\n" "docker $*"; }
      bash() { printf "%s\n" "bash $*"; }
      kind() { :; }
      kubectl() { :; }
      helm() { :; }
      main --with-pinned
    ' bash "$SUITE_SCRIPT" 2>&1
  )"

  if [[ "$output" != *"bash "*"/pinned-install.sh"* ]]; then
    echo "FAIL: suite with --with-pinned should run pinned-install.sh"
    echo "$output"
    exit 1
  fi
}

test_suite_rejects_unknown_argument() {
  local status=0
  local output
  output="$(
    RELEASE_CANDIDATE_SUITES_SOURCE_ONLY=1 bash -c '
      source "$1"
      main --invalid-option-xyz
    ' bash "$SUITE_SCRIPT" 2>&1
  )" || status=$?

  if [[ $status -eq 0 ]]; then
    echo "FAIL: suite should reject unknown argument"
    echo "$output"
    exit 1
  fi
  if [[ "$output" != *"FAIL: unknown argument"* ]]; then
    echo "FAIL: expected error message for unknown argument"
    echo "$output"
    exit 1
  fi
}

run_test "loads helpers" test_exports_main_without_running_flow
run_test "runs stages in order" test_suite_runs_stages_in_order
run_test "with-doctor runs doctor stages" test_suite_with_doctor_runs_doctor_stages
run_test "with-pinned runs pinned install" test_suite_with_pinned_runs_pinned_install
run_test "rejects unknown argument" test_suite_rejects_unknown_argument

echo "PASS: release candidate suite orchestration"
