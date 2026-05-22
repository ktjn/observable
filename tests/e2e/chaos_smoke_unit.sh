#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAOS_SCRIPT="$SCRIPT_DIR/../../scripts/chaos-smoke.sh"

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
    CHAOS_SMOKE_SOURCE_ONLY=1 bash -c '
      source "$1"
      declare -F main >/dev/null
      echo loaded
    ' bash "$CHAOS_SCRIPT" 2>&1
  )"

  assert_eq "loaded" "$output" "chaos-smoke.sh should load helpers without executing the probe"
}

test_chaos_restarts_storage_writer_and_waits_for_recovery() {
  local output
  local calls_file
  calls_file="$(mktemp)"
  output="$(
    CHAOS_SMOKE_SOURCE_ONLY=1 CALLS_FILE="$calls_file" bash -c '
      source "$1"
      docker() {
        printf "%s\n" "docker $*" >> "$CALLS_FILE"
      }
      source_smoke_helpers() { :; }
      curl() {
        if [[ "$*" == *"/health"* ]]; then
          return 0
        elif [[ "$*" == *"X-Tenant-ID: 00000000-0000-0000-0000-000000000001"* ]]; then
          printf "%s" "{\"spans\":[]}"
        else
          printf "%s" "{\"spans\":[{\"traceId\":\"abc\"}]}"
        fi
      }
      jq() {
        local filter="$2"
        local payload
        payload="$(cat)"
        if [[ "$filter" == ".spans | length" ]]; then
          if [[ "$payload" == *"traceId"* ]]; then
            echo 1
          else
            echo 0
          fi
        fi
      }
      sleep() { :; }
      TOKEN=test-token
      QUERY=http://query.test
      RUN_ID=12345
      CHAOS_SERVICE=storage-writer
      CHAOS_HEALTH_URL=http://localhost:4320/health
      send_trace_until_queryable() { echo "sent:$2"; }
      wait_for_json_count() { echo "wait:$1"; return 1; }
      main
    ' bash "$CHAOS_SCRIPT" 2>&1
  )"

  local calls
  calls="$(cat "$calls_file")"

  if [[ "$calls" != *"docker compose kill -s SIGKILL storage-writer"* ]]; then
    echo "FAIL: chaos probe should kill storage-writer"
    echo "$calls"
    exit 1
  fi

  if [[ "$calls" != *"docker compose up -d storage-writer"* ]]; then
    echo "FAIL: chaos probe should restart storage-writer"
    echo "$calls"
    exit 1
  fi
}

run_test "loads helpers" test_exports_main_without_running_flow
run_test "restarts storage-writer" test_chaos_restarts_storage_writer_and_waits_for_recovery

echo "PASS: chaos smoke probe"
