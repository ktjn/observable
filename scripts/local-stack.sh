#!/usr/bin/env bash
# Backing container stack + host-run services for the local gate.
#
# Only the stateful dependencies run in containers (ClickHouse, Redpanda,
# PostgreSQL); the Rust services run as host executables from target/debug,
# which `cargo nextest run` already builds for integration tests. This avoids
# the per-component Docker release builds that dominated scripts/local-ci.sh.
#
# Usage:
#   bash scripts/local-stack.sh up              # start + migrate backing stack (idempotent, reuses a running one)
#   bash scripts/local-stack.sh env             # print `export` lines for the test fixtures
#   bash scripts/local-stack.sh services-up     # start host services, wait for /health
#   bash scripts/local-stack.sh services-down   # stop host services
#   bash scripts/local-stack.sh smoke-test      # end-to-end smoke against host services
#   bash scripts/local-stack.sh perf-smoke      # latency baselines against host services
#   bash scripts/local-stack.sh down [--volumes]
#
# Environment:
#   LOCAL_STACK_BIN_DIR   service binaries (default: target/debug)
#   LOCAL_STACK_DIR       pid files and service logs (default: .local-ci/stack)
#   LOCAL_STACK_CONTAINER_RUNNER=1  run smoke runners in the verification image
#                         even when curl, jq and grpcurl are installed
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BIN_DIR=${LOCAL_STACK_BIN_DIR:-$REPO_ROOT/target/debug}
STATE_DIR=${LOCAL_STACK_DIR:-$REPO_ROOT/.local-ci/stack}
CH_USER=${CH_USER:-default}
CH_PASSWORD=${CH_PASSWORD:-observable}
PG_USER=${PG_USER:-observable}
PG_PASSWORD=${PG_PASSWORD:-observable}
PG_DB=${PG_DB:-observable}

BACKING=(clickhouse redpanda postgres)
SETUP=(clickhouse-setup postgres-setup redpanda-setup)
# Start order matters only for health dependencies: ingest-gateway validates
# keys through auth-service; query-api proxies to admin-service.
SERVICES=(auth-service admin-service alert-evaluator ingest-gateway storage-writer stream-processor query-api)

service_port() {
  case $1 in
    auth-service)     echo 4319 ;;
    storage-writer)   echo 4320 ;;
    stream-processor) echo 4323 ;;
    ingest-gateway)   echo 4318 ;;
    query-api)        echo 8090 ;;
    admin-service)    echo 4324 ;;
    alert-evaluator)  echo 4322 ;;
  esac
}

# Mirrors the `environment:` blocks in docker-compose.yml with service
# hostnames replaced by localhost. Zitadel is not started: the smoke path
# authenticates with API keys, and auth-service only needs the OIDC settings
# to be present.
service_env() {
  local common=(
    OBSERVABLE_ENV=dev
    OBSERVABLE_SELF_OBSERVABILITY_MODE=${OBSERVABLE_SELF_OBSERVABILITY_MODE:-self}
    OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=http://localhost:4317
    OBSERVABLE_SELF_OBSERVABILITY_BEARER_TOKEN=observable-api-key-0000
    RUST_LOG=${RUST_LOG:-info}
  )
  local ch=(CLICKHOUSE_URL=http://localhost:8123 CLICKHOUSE_USER=$CH_USER CLICKHOUSE_PASSWORD=$CH_PASSWORD)
  local internal=(INTERNAL_SERVICE_TOKEN=${INTERNAL_SERVICE_TOKEN:-dev-internal-token-0000} AUTH_SERVICE_URL=http://localhost:4319)
  printf '%s\n' "${common[@]}"
  case $1 in
    auth-service)
      printf '%s\n' \
        DATABASE_URL=postgres://observable_auth:observable_auth@localhost:5432/observable \
        AUTH_DATABASE_URL=postgres://observable_auth:observable_auth@localhost:5432/observable \
        AUTH_SERVICE_PORT=4319 \
        ZITADEL_ISSUER=http://localhost:8082 \
        ZITADEL_API_BASE=http://localhost:8082 \
        ZITADEL_CLIENT_ID=local-ci \
        "ZITADEL_REDIRECT_URI=http://localhost:${FRONTEND_PORT:-5173}/auth/callback" \
        SESSION_SECRET=${SESSION_SECRET:-dev-session-secret-for-observable-00} ;;
    storage-writer)
      printf '%s\n' "${ch[@]}" STORAGE_WRITER_PORT=4320 REDPANDA_BROKERS=localhost:9092 \
        NORMALIZED_TOPIC=telemetry.normalized.v1 ;;
    stream-processor)
      printf '%s\n' REDPANDA_BROKERS=localhost:9092 INGEST_TOPIC=telemetry.raw.v1 \
        NORMALIZED_TOPIC=telemetry.normalized.v1 STREAM_PROCESSOR_PLATFORM_PORT=4323 ;;
    ingest-gateway)
      printf '%s\n' INGEST_GATEWAY_HTTP_JSON_PORT=4318 INGEST_GATEWAY_GRPC_PORT=4317 \
        INGEST_GATEWAY_PLATFORM_PORT=4321 REDPANDA_BROKERS=localhost:9092 INGEST_TOPIC=telemetry.raw.v1 \
        DEPLOYMENT_MARKERS_TOPIC=deployment.markers.v1 AUTH_SERVICE_URL=http://localhost:4319 ;;
    query-api)
      printf '%s\n' "${ch[@]}" "${internal[@]}" QUERY_API_PORT=8090 \
        ADMIN_SERVICE_URL=http://localhost:4324 ALERT_EVALUATOR_URL=http://localhost:4322 ;;
    admin-service)
      printf '%s\n' "${ch[@]}" "${internal[@]}" ADMIN_SERVICE_PORT=4324 \
        DATABASE_URL=postgres://observable_control:observable_control@localhost:5432/observable \
        CONTROL_DATABASE_URL=postgres://observable_control:observable_control@localhost:5432/observable \
        REDPANDA_BROKERS=localhost:9092 DEPLOYMENT_MARKERS_TOPIC=deployment.markers.v1 ;;
    alert-evaluator)
      printf '%s\n' "${ch[@]}" "${internal[@]}" ALERT_EVALUATOR_PORT=4322 ALERT_EVAL_INTERVAL_SECONDS=60 \
        DATABASE_URL=postgres://observable_alerting:observable_alerting@localhost:5432/observable \
        ALERTING_DATABASE_URL=postgres://observable_alerting:observable_alerting@localhost:5432/observable ;;
  esac
}

print_env() {
  cat <<EOF
export OBSERVABLE_TEST_POSTGRES_URL=postgres://$PG_USER:$PG_PASSWORD@127.0.0.1:5432
export OBSERVABLE_TEST_CLICKHOUSE_URL=http://127.0.0.1:8123
export OBSERVABLE_TEST_CLICKHOUSE_USER=$CH_USER
export OBSERVABLE_TEST_CLICKHOUSE_PASSWORD=$CH_PASSWORD
export OBSERVABLE_TEST_KAFKA_BROKERS=127.0.0.1:9092
EOF
}

psql_admin() {
  docker compose exec -T postgres psql -v ON_ERROR_STOP=1 -qAt -U "$PG_USER" -d "$PG_DB" "$@"
}

stack_up() {
  local started
  started=$(date +%s)
  docker compose up -d --wait "${BACKING[@]}"

  # One-shot setup containers are independent of each other; run them
  # concurrently. All three are idempotent, so a reused stack is re-migrated.
  local pids=() names=() i status=0
  mkdir -p "$STATE_DIR"
  for i in "${SETUP[@]}"; do
    docker compose run --rm --no-deps "$i" >"$STATE_DIR/$i.log" 2>&1 &
    pids+=("$!")
    names+=("$i")
  done
  for i in "${!pids[@]}"; do
    if ! wait "${pids[$i]}"; then
      echo "FAIL ${names[$i]} (log: $STATE_DIR/${names[$i]}.log)"
      tail -n 40 "$STATE_DIR/${names[$i]}.log"
      status=1
    fi
  done
  [[ $status -eq 0 ]] || return "$status"

  # Drop per-test databases left behind by earlier runs. Template databases
  # (obs_tmpl_*) are keyed by migration content and kept for reuse.
  local stale
  stale=$(psql_admin -c "SELECT datname FROM pg_database WHERE datname LIKE 'test\_%'") || return 1
  if [[ -n "$stale" ]]; then
    while IFS= read -r db; do
      psql_admin -c "DROP DATABASE IF EXISTS \"$db\" WITH (FORCE)" >/dev/null
    done <<<"$stale"
  fi
  echo "backing stack ready in $(( $(date +%s) - started ))s"
}

stack_down() {
  services_down
  if [[ ${1:-} == --volumes ]]; then
    docker compose down --volumes --remove-orphans
  else
    docker compose stop "${BACKING[@]}"
  fi
}

port_busy() {
  (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null
}

wait_healthy() {
  local name=$1 port=$2 i
  for i in $(seq 1 120); do
    if curl -fsS "http://localhost:$port/health" >/dev/null 2>&1; then
      return 0
    fi
    if ! kill -0 "$(cat "$STATE_DIR/$name.pid")" 2>/dev/null; then
      echo "FAIL $name exited during start-up (log: $STATE_DIR/$name.log)"
      tail -n 40 "$STATE_DIR/$name.log"
      return 1
    fi
    sleep 0.25
  done
  echo "FAIL $name did not report healthy on :$port within 30s (log: $STATE_DIR/$name.log)"
  tail -n 40 "$STATE_DIR/$name.log"
  return 1
}

services_up() {
  mkdir -p "$STATE_DIR"
  local name port
  for name in "${SERVICES[@]}"; do
    if [[ ! -x "$BIN_DIR/$name" ]]; then
      echo "FAIL missing $BIN_DIR/$name (build with: cargo build --workspace --bins)"
      return 1
    fi
    port=$(service_port "$name")
    if port_busy "$port"; then
      echo "FAIL port $port for $name is in use (is the containerized dev stack running? docker compose stop $name)"
      return 1
    fi
  done
  for name in "${SERVICES[@]}"; do
    # setsid: own process group, so services_down can stop the whole tree.
    mapfile -t env_vars < <(service_env "$name")
    setsid env "${env_vars[@]}" "$BIN_DIR/$name" >"$STATE_DIR/$name.log" 2>&1 </dev/null &
    echo "$!" >"$STATE_DIR/$name.pid"
  done
  for name in "${SERVICES[@]}"; do
    wait_healthy "$name" "$(service_port "$name")" || return 1
  done
  echo "host services healthy: ${SERVICES[*]}"
}

services_down() {
  local name pid
  for name in "${SERVICES[@]}"; do
    [[ -f "$STATE_DIR/$name.pid" ]] || continue
    pid=$(cat "$STATE_DIR/$name.pid")
    kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
    rm -f "$STATE_DIR/$name.pid"
  done
}

# Smoke runners. With curl, jq and grpcurl on the host they run directly
# (no image build); otherwise inside the verification-tools image, reaching
# the host services through the host-gateway alias. The OTel Collector is
# always a container (no Rust code in either image).
host_runner() {
  [[ ${LOCAL_STACK_CONTAINER_RUNNER:-0} -eq 0 ]] \
    && command -v curl >/dev/null && command -v jq >/dev/null && command -v grpcurl >/dev/null
}

smoke_test() {
  docker compose --profile host-verification up -d --wait otel-collector-host >/dev/null 2>&1 \
    || docker compose --profile host-verification up -d otel-collector-host
  local status=0
  if host_runner; then
    OTLP_PROTO_DIR="$REPO_ROOT/proto/otlp" COLLECTOR_URL=http://localhost:14318 \
      COLLECTOR_GRPC_URL=http://localhost:14317 bash tests/e2e/smoke_test.sh || status=$?
  else
    docker compose --profile host-verification run --rm smoke-test-host || status=$?
  fi
  docker compose --profile host-verification rm -sf otel-collector-host >/dev/null 2>&1 || true
  return "$status"
}

perf_smoke() {
  if host_runner; then
    SAMPLE_COUNT=20 bash scripts/perf-smoke.sh
  else
    docker compose --profile host-verification run --rm perf-smoke-host
  fi
}

case ${1:-} in
  up)            stack_up ;;
  env)           print_env ;;
  services-up)   services_up ;;
  services-down) services_down ;;
  smoke-test)    smoke_test ;;
  perf-smoke)    perf_smoke ;;
  down)          shift; stack_down "$@" ;;
  *) sed -n '2,23p' "${BASH_SOURCE[0]}"; exit 2 ;;
esac
