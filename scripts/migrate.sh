#!/bin/bash
set -euo pipefail
# Apply ClickHouse, PostgreSQL, and Redpanda migrations/setup
docker compose run --rm clickhouse-setup
docker compose run --rm postgres-setup
docker compose run --rm redpanda-setup
