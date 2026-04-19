#!/usr/bin/env bash
set -euo pipefail

NAMESPACE="${1:-observable}"

kubectl get all --namespace "$NAMESPACE" || true
kubectl describe pods --namespace "$NAMESPACE" || true
for deploy in auth-service ingest-gateway stream-processor storage-writer query-api alert-evaluator; do
  echo "=== $deploy logs ==="
  kubectl logs deployment/$deploy --namespace "$NAMESPACE" --tail=50 || true
done
