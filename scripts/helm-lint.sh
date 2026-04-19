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

echo "==> Checking helm version"
helm version --short

echo ""
echo "==> Linting observable-common (library chart)"
helm lint "$COMMON_CHART"

echo ""
echo "==> Resolving dependencies for observable-infra"
helm repo add cloudnative-pg https://cloudnative-pg.github.io/charts
helm repo add redpanda https://charts.redpanda.com
helm repo add openfga https://openfga.github.io/helm-charts
helm repo update
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
helm template observable-dev "$APP_CHART" \
  --namespace observable \
  --debug \
  --dry-run \
  > /dev/null  # render succeeds; suppress output for CI brevity

echo ""
echo "==> Template rendering with example override"
helm template observable-dev "$APP_CHART" \
  --namespace observable \
  --set global.image.tag=abc123 \
  --set services.authService.replicas=2 \
  --dry-run \
  > /dev/null

echo ""
echo "All Helm lint and template checks passed."
