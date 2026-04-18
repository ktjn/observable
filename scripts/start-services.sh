#!/bin/bash
set -e
# Build and start all Rust services in detached mode
docker compose up -d auth-service storage-writer stream-processor ingest-gateway query-api
