#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_SCRIPT="$SCRIPT_DIR/smoke_test.sh"

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

test_exports_wait_for_json_count_without_running_main() {
  local output
  output="$(
    SMOKE_TEST_SOURCE_ONLY=1 bash -c '
      source "$1"
      declare -F wait_for_json_count >/dev/null
      echo loaded
    ' bash "$SMOKE_SCRIPT" 2>&1
  )"
  assert_eq "loaded" "$output" "smoke_test.sh should load helper definitions without executing the smoke flow"
}

test_wait_for_json_count_retries_until_rows_exist() {
  local output
  local attempts_file
  attempts_file="$(mktemp)"
  output="$(
    ATTEMPTS_FILE="$attempts_file" SMOKE_TEST_SOURCE_ONLY=1 bash -c '
      source "$1"
      : > "$ATTEMPTS_FILE"
      curl() {
        local attempts
        attempts=$(wc -l < "$ATTEMPTS_FILE")
        attempts=$((attempts + 1))
        printf "x\n" >> "$ATTEMPTS_FILE"
        if [[ $attempts -lt 3 ]]; then
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
      TENANT_ID=test-tenant
      result="$(wait_for_json_count "trace detail" "http://example.test" ".spans | length" 5 0)"
      printf "%s\n%s\n" "$(wc -l < "$ATTEMPTS_FILE")" "$result"
    ' bash "$SMOKE_SCRIPT" 2>&1
  )"

  local attempts
  local message
  attempts="$(echo "$output" | sed -n '1p')"
  message="$(echo "$output" | sed -n '2p')"

  assert_eq "3" "$attempts" "wait_for_json_count should keep polling until results appear"
  assert_eq " OK (trace detail) - 1 record(s)" "$message" "wait_for_json_count should report success with the observed count"
  rm -f "$attempts_file"
}

test_assert_http_status_checks_expected_code() {
  local output
  output="$(
    SMOKE_TEST_SOURCE_ONLY=1 bash -c '
      source "$1"
      curl() {
        printf "%s" "403"
      }
      assert_http_status "viewer ingest rejected" "403" -X POST http://example.test/v1/traces
    ' bash "$SMOKE_SCRIPT" 2>&1
  )"

  assert_eq " OK (viewer ingest rejected)" "$output" "assert_http_status should report success when the expected code matches"
}

test_send_trace_until_queryable_retries_ingest() {
  local output
  output="$(
    SMOKE_TEST_SOURCE_ONLY=1 bash -c '
      source "$1"
      attempts_file="$(mktemp)"
      : > "$attempts_file"
      curl() {
        printf "post\n" >> "$attempts_file"
        printf "%s" "{\"partialSuccess\":{}}"
      }
      wait_for_json_count() {
        wait_calls="${wait_calls:-0}"
        wait_calls=$((wait_calls + 1))
        if [[ "$wait_calls" -lt 2 ]]; then
          return 1
        fi
        echo " OK (detail) - 1 record(s)"
      }
      INGEST=http://example.test
      TRACE_ID=test-trace-id
      SERVICE_NAME=test-service
      TENANT_ID=test-tenant
      send_trace_until_queryable "{\"resourceSpans\":[]}" 2 >/dev/null
      wc -l < "$attempts_file"
      rm -f "$attempts_file"
    ' bash "$SMOKE_SCRIPT" 2>&1
  )"

  assert_eq "2" "$output" "send_trace_until_queryable should retry trace ingest when detail is not yet queryable"
}

run_test "loads helper definitions" test_exports_wait_for_json_count_without_running_main
run_test "retries until rows exist" test_wait_for_json_count_retries_until_rows_exist
run_test "checks expected HTTP status" test_assert_http_status_checks_expected_code
run_test "retries trace ingest" test_send_trace_until_queryable_retries_ingest

echo "PASS: smoke_test polling helper"
