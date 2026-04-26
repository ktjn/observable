#!/usr/bin/env bash
# CI/CD helper for Observable deployment markers.
#
# Usage:
#   # Start a deployment (prints deployment_id to stdout):
#   DEPLOYMENT_ID=$(bash scripts/deployment-marker.sh start \
#     --service shop-api --env staging --version v1.3.0 \
#     --deployed-by ci-bot --commit abc123)
#
#   # Finish a deployment:
#   bash scripts/deployment-marker.sh finish \
#     --id "$DEPLOYMENT_ID" --status success
#
# Environment variables:
#   OBSERVABLE_URL        Base URL of the Observable ingest-gateway Platform API
#                         (default: http://localhost:4321)
#   OBSERVABLE_API_KEY    Bearer token for the Authorization header
#                         (default: dev-api-key-0000 for local dev)

set -euo pipefail

BASE_URL="${OBSERVABLE_URL:-http://localhost:4321}"
API_KEY="${OBSERVABLE_API_KEY:-dev-api-key-0000}"
SUBCOMMAND="${1:-}"
shift || true

case "$SUBCOMMAND" in
  start)
    SERVICE_NAME=""
    ENVIRONMENT=""
    SERVICE_VERSION=""
    DEPLOYED_BY=""
    COMMIT_SHA=""

    while [[ $# -gt 0 ]]; do
      case "$1" in
        --service)     SERVICE_NAME="$2";    shift 2 ;;
        --env)         ENVIRONMENT="$2";     shift 2 ;;
        --version)     SERVICE_VERSION="$2"; shift 2 ;;
        --deployed-by) DEPLOYED_BY="$2";     shift 2 ;;
        --commit)      COMMIT_SHA="$2";      shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
      esac
    done

    if [[ -z "$SERVICE_NAME" || -z "$ENVIRONMENT" || -z "$SERVICE_VERSION" ]]; then
      echo "ERROR: --service, --env, and --version are required for start" >&2
      exit 1
    fi

    PAYLOAD=$(printf '{"service_name":"%s","environment":"%s","service_version":"%s","deployed_by":"%s","commit_sha":"%s"}' \
      "$SERVICE_NAME" "$ENVIRONMENT" "$SERVICE_VERSION" "$DEPLOYED_BY" "$COMMIT_SHA")

    RESPONSE=$(curl -sf -X POST "$BASE_URL/v1/deployments" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $API_KEY" \
      -d "$PAYLOAD")

    echo "$RESPONSE" | grep -o '"deployment_id":"[^"]*"' | cut -d'"' -f4
    ;;

  finish)
    DEPLOYMENT_ID=""
    STATUS=""

    while [[ $# -gt 0 ]]; do
      case "$1" in
        --id)     DEPLOYMENT_ID="$2"; shift 2 ;;
        --status) STATUS="$2";        shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
      esac
    done

    if [[ -z "$DEPLOYMENT_ID" || -z "$STATUS" ]]; then
      echo "ERROR: --id and --status are required for finish" >&2
      exit 1
    fi

    ALLOWED="success failed rolled_back"
    if ! echo "$ALLOWED" | grep -qw "$STATUS"; then
      echo "ERROR: --status must be one of: $ALLOWED" >&2
      exit 1
    fi

    curl -sf -X PATCH "$BASE_URL/v1/deployments/$DEPLOYMENT_ID" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $API_KEY" \
      -d "{\"status\":\"$STATUS\"}"

    echo "Deployment $DEPLOYMENT_ID marked $STATUS"
    ;;

  *)
    echo "Usage: $0 {start|finish} [options]" >&2
    exit 1
    ;;
esac
