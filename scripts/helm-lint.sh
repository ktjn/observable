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
helm repo add --force-update openfga https://openfga.github.io/helm-charts
helm repo add --force-update open-telemetry https://open-telemetry.github.io/opentelemetry-helm-charts
helm repo update

echo ""
echo "==> Building dependencies for observable-infra (from Chart.lock)"
helm dependency build "$INFRA_CHART"

echo ""
echo "==> Linting observable-infra"
helm lint "$INFRA_CHART"

echo ""
echo "==> Building dependencies for observable (from Chart.lock)"
helm dependency build "$APP_CHART"

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

if grep -q "observable-services" <<<"$FULL_RENDER"; then
  echo "ERROR: observable chart must not reference the shared observable-services image" >&2
  exit 1
fi

for component in auth control ingest process store-clickhouse query alerting web; do
  # The rendered image must be the manifest-pinned version, not just any tag: a
  # template could otherwise hardcode a tag and diverge from the manifest while
  # scripts/check-distribution-manifest.sh (which reads values.yaml) still passes.
  version="$(bash "$SCRIPT_DIR/component-version.sh" "$component")"
  escaped="${version//./\\.}"
  if ! grep -qE "image: (\S+/)?observable-${component}:${escaped}([[:space:]]|$)" <<<"$FULL_RENDER"; then
    echo "ERROR: observable chart must render observable-${component}:${version} (manifest pin)" >&2
    exit 1
  fi
done

echo ""
echo "==> Template rendering with example override"
helm template observable-dev "$APP_CHART" \
  --namespace observable \
  --set services.authService.image.tag=abc123 \
  --set services.authService.replicas=2 \
  --dry-run \
  > /dev/null

echo ""
echo "==> Building dependencies for observable-testbench (from Chart.lock)"
helm dependency build "$TESTBENCH_CHART"

echo ""
echo "==> Linting observable-testbench"
helm lint "$TESTBENCH_CHART"

echo ""
echo "All Helm lint and template checks passed."
