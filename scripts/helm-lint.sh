#!/usr/bin/env bash
# Lint and template-render the Observable Helm charts.
# Runnable locally and in CI (ADR-019).
#
# Usage:
#   bash scripts/helm-lint.sh
#
# Prerequisites: helm >= 3.0 on PATH.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

COMMON_CHART="$REPO_ROOT/charts/observable-common"
INFRA_CHART="$REPO_ROOT/charts/observable-infra"
APP_CHART="$REPO_ROOT/charts/observable"
TESTBENCH_CHART="$REPO_ROOT/charts/observable-testbench"

echo "==> Checking helm version"
helm version --short

echo ""
echo "==> Linting observable-common (library chart)"
helm lint "$COMMON_CHART"

echo ""
echo "==> Adding Helm repositories"
helm repo add cloudnative-pg https://cloudnative-pg.github.io/charts
helm repo add openfga https://openfga.github.io/helm-charts
helm repo add open-telemetry https://open-telemetry.github.io/opentelemetry-helm-charts
helm repo update

echo ""
echo "==> Resolving dependencies for observable-infra"
helm dependency update "$INFRA_CHART"

echo ""
echo "==> Linting observable-infra"
helm lint "$INFRA_CHART"

echo ""
echo "==> Resolving dependencies for observable"
helm dependency update "$APP_CHART"

echo ""
echo "==> Linting observable"
helm lint "$APP_CHART"

echo ""
echo "==> Template rendering (dry-run) — full manifest dump"
FULL_RENDER="$(helm template observable-dev "$APP_CHART" \
  --namespace observable \
  --debug \
  --dry-run)"

if ! grep -q "kind: Deployment" <<<"$FULL_RENDER" || ! grep -q "name: frontend" <<<"$FULL_RENDER"; then
  echo "ERROR: observable chart must render a frontend Deployment" >&2
  exit 1
fi

if ! grep -q "kind: Service" <<<"$FULL_RENDER" || ! grep -q "name: frontend" <<<"$FULL_RENDER"; then
  echo "ERROR: observable chart must render a frontend Service" >&2
  exit 1
fi

echo ""
echo "==> Template rendering with example override"
helm template observable-dev "$APP_CHART" \
  --namespace observable \
  --set global.image.tag=abc123 \
  --set services.authService.replicas=2 \
  --dry-run \
  > /dev/null

echo ""
echo "==> Resolving dependencies for observable-testbench"
helm dependency update "$TESTBENCH_CHART"

echo ""
echo "==> Linting observable-testbench"
helm lint "$TESTBENCH_CHART"

echo ""
echo "All Helm lint and template checks passed."
