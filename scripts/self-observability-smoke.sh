#!/usr/bin/env bash
# Focused self-observability route smoke check.
# Verifies one service path, one frontend path, and one infrastructure path
# all carry the configured destination and system tenant metadata.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INFRA_CHART="$REPO_ROOT/charts/observable-infra"

MODE="${OBSERVABLE_SELF_OBSERVABILITY_MODE:-observer_instance}"
ENDPOINT="${OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT:-https://observer.example/otlp}"
TENANT="${OBSERVABLE_SELF_OBSERVABILITY_TENANT:-system}"
ENVIRONMENT="${OBSERVABLE_DEPLOYMENT_ENVIRONMENT:-staging}"
CLUSTER="${OBSERVABLE_CLUSTER:-kind-observable}"
RELEASE="${OBSERVABLE_RELEASE:-observable-smoke}"

log() {
  echo ""
  echo "==> $*"
}

check_compose() {
  log "Verifying Docker Compose service and frontend routes"
  local compose_config
  compose_config="$(docker compose -f "$REPO_ROOT/docker-compose.yml" config 2>/dev/null)"
  printf '%s' "$compose_config" | grep -q "OBSERVABLE_SELF_OBSERVABILITY_MODE: $MODE"
  printf '%s' "$compose_config" | grep -q "OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT: $ENDPOINT"
  printf '%s' "$compose_config" | grep -q "OBSERVABLE_SELF_OBSERVABILITY_TENANT: $TENANT"
  printf '%s' "$compose_config" | grep -q "VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE: $MODE"
  printf '%s' "$compose_config" | grep -q "VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT: $ENDPOINT"
  printf '%s' "$compose_config" | grep -q "VITE_OBSERVABLE_SELF_OBSERVABILITY_TENANT: $TENANT"
}

check_frontend_tests() {
  log "Running focused frontend self-observability tests"
  npm --prefix "$REPO_ROOT/apps/frontend" test -- --run src/lib/selfObservability.test.ts src/lib/selfObservabilityRuntime.test.ts
}

check_infra_collector() {
  log "Rendering infrastructure collector route"
  local rendered
  rendered="$(helm template observable-infra "$INFRA_CHART" \
    --namespace observable \
    --set selfObservability.mode="$MODE" \
    --set selfObservability.otlpEndpoint="$ENDPOINT" \
    --set selfObservability.tenant="$TENANT" \
    --set selfObservability.deploymentEnvironment="$ENVIRONMENT" \
    --set selfObservability.cluster="$CLUSTER" \
    --set selfObservability.release="$RELEASE")"
  printf '%s' "$rendered" | grep -q 'name: observable-self-observability-gateway'
  printf '%s' "$rendered" | grep -q 'name: observable-self-observability-node-agent'
  printf '%s' "$rendered" | grep -q "value: \"$TENANT\""
  printf '%s' "$rendered" | grep -q "endpoint: \"$ENDPOINT\""
  printf '%s' "$rendered" | grep -q "observable.com/self-observability-mode: \"$MODE\""
}

check_compose
check_frontend_tests
check_infra_collector

log "Self-observability route smoke checks passed"
