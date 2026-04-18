#!/usr/bin/env bash
# Start all application services in Docker Compose.
# Requires the Compose dependency stack and migrations to be ready.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_ARGS=()
if [ -f "$REPO_ROOT/.env.local" ]; then
  ENV_ARGS=(--env-file "$REPO_ROOT/.env.local")
fi

if command -v powershell.exe >/dev/null 2>&1; then
  powershell.exe -NoProfile -Command \
    "Get-Process auth-service,storage-writer,stream-processor,ingest-gateway,query-api -ErrorAction SilentlyContinue | Stop-Process -Force" \
    >/dev/null 2>&1 || true
fi

docker compose -f "$REPO_ROOT/docker-compose.yml" "${ENV_ARGS[@]}" --profile services up -d --build \
  auth-service \
  storage-writer \
  stream-processor \
  ingest-gateway \
  query-api

echo "Application services started in Docker Compose."
