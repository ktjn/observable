#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_SCRIPT="$SCRIPT_DIR/smoke_test.sh"
POSTGRES_MIGRATIONS_DIR="$SCRIPT_DIR/../../migrations/postgres"
CLICKHOUSE_MIGRATIONS_DIR="$SCRIPT_DIR/../../migrations/clickhouse"
MIGRATE_SCRIPT="$SCRIPT_DIR/../../scripts/migrate.sh"

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
          printf "%s" "{\"spans\":[]}" > /tmp/smoke_body
        else
          printf "%s" "{\"spans\":[{\"traceId\":\"abc\"}]}" > /tmp/smoke_body
        fi
        printf "%s" "200"
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

test_local_smoke_defaults_match_seeded_setup() {
  local output
  output="$(
    SMOKE_TEST_SOURCE_ONLY=1 bash -c '
      source "$1"
      printf "%s\n%s\n" "$TOKEN" "$TENANT_ID"
    ' bash "$SMOKE_SCRIPT" 2>&1
  )"

  local token
  local tenant
  token="$(echo "$output" | sed -n '1p')"
  tenant="$(echo "$output" | sed -n '2p')"

  assert_eq "dev-api-key-0000" "$token" "smoke test should use the seeded local dev API key"
  assert_eq "00000000-0000-0000-0000-000000000002" "$tenant" "smoke test should use the dev-tenant (migration 017 moved dev-key to ...002)"
}

test_postgres_migrations_seed_local_setup() {
  local tenant_migration="$POSTGRES_MIGRATIONS_DIR/001_create_tenants.sql"
  local key_migration="$POSTGRES_MIGRATIONS_DIR/002_create_api_keys.sql"

  if ! grep -q "00000000-0000-0000-0000-000000000001" "$tenant_migration"; then
    echo "FAIL: local dev tenant seed is missing"
    exit 1
  fi

  if ! grep -q "dev-api-key-0000" "$key_migration"; then
    echo "FAIL: local dev API key seed comment is missing"
    exit 1
  fi

  if ! grep -q "e18f3d8fb3eb31a042e4a55877e0276960294d0980b8076efaac30dabdbbf67b" "$key_migration"; then
    echo "FAIL: local dev API key hash seed is missing"
    exit 1
  fi
}

test_postgres_role_migration_is_rerunnable() {
  local role_migration="$POSTGRES_MIGRATIONS_DIR/006_add_role_to_api_keys.sql"

  if ! grep -q "ADD COLUMN IF NOT EXISTS role" "$role_migration"; then
    echo "FAIL: api_keys role migration must be rerunnable for local setup volumes"
    exit 1
  fi
}

test_clickhouse_nanosecond_ttl_uses_datetime_cast() {
  local migration="$CLICKHOUSE_MIGRATIONS_DIR/004_create_span_events.sql"

  if grep -q "TTL fromUnixTimestamp64Nano" "$migration"; then
    echo "FAIL: span_events TTL must cast DateTime64 to DateTime for ClickHouse 24.3"
    exit 1
  fi

  if ! grep -q "TTL toDateTime(fromUnixTimestamp64Nano(timestamp_unix_nano)) + INTERVAL 14 DAY" "$migration"; then
    echo "FAIL: span_events TTL DateTime cast is missing"
    exit 1
  fi
}

test_migrate_script_propagates_setup_failures() {
  if grep -q "docker compose up clickhouse-setup postgres-setup redpanda-setup" "$MIGRATE_SCRIPT"; then
    echo "FAIL: migrate.sh must not use plain docker compose up because setup container failures can be masked"
    exit 1
  fi

  if ! grep -q "docker compose run --rm clickhouse-setup" "$MIGRATE_SCRIPT"; then
    echo "FAIL: migrate.sh must run clickhouse-setup with an exit code checked by set -e"
    exit 1
  fi

  if ! grep -q "docker compose run --rm postgres-setup" "$MIGRATE_SCRIPT"; then
    echo "FAIL: migrate.sh must run postgres-setup with an exit code checked by set -e"
    exit 1
  fi
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

test_smoke_verifies_metric_points_after_ingest() {
  if ! grep -q "Verifying metric series is queryable" "$SMOKE_SCRIPT"; then
    echo "FAIL: smoke_test.sh must query metric series after sending a metric"
    exit 1
  fi

  if ! grep -q "Verifying metric points are queryable" "$SMOKE_SCRIPT"; then
    echo "FAIL: smoke_test.sh must query metric points after finding the metric series"
    exit 1
  fi
}

test_local_ci_has_integration_test_stage() {
  local ci_script="$SCRIPT_DIR/../../scripts/local-ci.sh"

  if ! grep -q "cargo test --workspace --tests" "$ci_script"; then
    echo "FAIL: local-ci.sh must have an explicit integration-test stage (cargo test --workspace --tests)"
    exit 1
  fi
}

test_local_ci_has_fmt_gate() {
  local ci_script="$SCRIPT_DIR/../../scripts/local-ci.sh"

  if ! grep -q "cargo fmt --all -- --check" "$ci_script"; then
    echo "FAIL: local-ci.sh must have a cargo fmt check gate"
    exit 1
  fi
}

test_local_ci_has_clippy_gate() {
  local ci_script="$SCRIPT_DIR/../../scripts/local-ci.sh"

  if ! grep -q "cargo clippy --workspace --all-targets -- -D warnings" "$ci_script"; then
    echo "FAIL: local-ci.sh must have a cargo clippy gate"
    exit 1
  fi
}

test_local_ci_has_unit_test_gate() {
  local ci_script="$SCRIPT_DIR/../../scripts/local-ci.sh"

  if ! grep -q "cargo test --workspace --lib --bins" "$ci_script"; then
    echo "FAIL: local-ci.sh must have a cargo unit test gate (--lib --bins)"
    exit 1
  fi
}

test_grpc_handlers_suppress_self_telemetry_spans() {
  local base="$SCRIPT_DIR/../../services/ingest-gateway/src/grpc"

  for handler in log trace metric; do
    if ! grep -q "is_self_telemetry_env" "$base/${handler}.rs"; then
      echo "FAIL: ingest-gateway grpc/${handler}.rs must use is_self_telemetry_env() for span suppression"
      exit 1
    fi
  done
}

test_stream_processor_uses_telemetry_constant() {
  local main="$SCRIPT_DIR/../../services/stream-processor/src/main.rs"

  if grep -q '"observable"' "$main"; then
    echo "FAIL: stream-processor/main.rs must not use raw \"observable\" string (use SELF_TELEMETRY_ENV or is_self_telemetry_env)"
    exit 1
  fi
}

test_storage_writer_uses_telemetry_constant() {
  local main="$SCRIPT_DIR/../../services/storage-writer/src/main.rs"

  # Check that the TraceLayer span suppression uses the constant, not a raw literal.
  # We look for SELF_TELEMETRY_ENV in the span-suppression context.
  if ! grep -q "SELF_TELEMETRY_ENV" "$main"; then
    echo "FAIL: storage-writer/main.rs must use SELF_TELEMETRY_ENV for span suppression (not a raw string)"
    exit 1
  fi
}

test_storage_writer_has_write_buffer() {
  local buf="$SCRIPT_DIR/../../services/storage-writer/src/buffer.rs"

  if [ ! -f "$buf" ]; then
    echo "FAIL: services/storage-writer/src/buffer.rs must exist (async write buffer for ClickHouse insert efficiency)"
    exit 1
  fi
}

test_frontend_has_service_filter_hook() {
  local hook="$SCRIPT_DIR/../../apps/frontend/src/hooks/useGlobalServiceFilter.ts"

  if [ ! -f "$hook" ]; then
    echo "FAIL: apps/frontend/src/hooks/useGlobalServiceFilter.ts must exist (URL-persisted service filter hook)"
    exit 1
  fi
}

run_test "loads helper definitions" test_exports_wait_for_json_count_without_running_main
run_test "retries until rows exist" test_wait_for_json_count_retries_until_rows_exist
run_test "checks expected HTTP status" test_assert_http_status_checks_expected_code
run_test "local defaults match seeded setup" test_local_smoke_defaults_match_seeded_setup
run_test "postgres migrations seed local setup" test_postgres_migrations_seed_local_setup
run_test "postgres role migration is rerunnable" test_postgres_role_migration_is_rerunnable
run_test "clickhouse nanosecond TTL uses DateTime cast" test_clickhouse_nanosecond_ttl_uses_datetime_cast
run_test "migrate script propagates setup failures" test_migrate_script_propagates_setup_failures
run_test "retries trace ingest" test_send_trace_until_queryable_retries_ingest
run_test "verifies metric point readback" test_smoke_verifies_metric_points_after_ingest
run_test "local-ci has integration-test stage" test_local_ci_has_integration_test_stage
run_test "local-ci has fmt gate" test_local_ci_has_fmt_gate
run_test "local-ci has clippy gate" test_local_ci_has_clippy_gate
run_test "local-ci has unit-test gate" test_local_ci_has_unit_test_gate
run_test "grpc handlers suppress self-telemetry spans" test_grpc_handlers_suppress_self_telemetry_spans
run_test "stream-processor uses telemetry constant" test_stream_processor_uses_telemetry_constant
run_test "storage-writer uses telemetry constant" test_storage_writer_uses_telemetry_constant
run_test "storage-writer has write buffer" test_storage_writer_has_write_buffer
run_test "frontend has service filter hook" test_frontend_has_service_filter_hook

echo "PASS: smoke_test polling helper"
