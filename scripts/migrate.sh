#!/bin/bash
set -e
# Apply ClickHouse, PostgreSQL, and Redpanda migrations/setup
docker compose up clickhouse-setup postgres-setup redpanda-setup
