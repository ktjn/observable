#!/usr/bin/env bash
# Start all compiled services in the background.
# Usage: scripts/start-services.sh [build-dir]
# Default build-dir is ./target/release (run `cargo build --release` first).
set -euo pipefail

BUILD_DIR="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/target/release}"

echo "Starting services from $BUILD_DIR ..."
"$BUILD_DIR/auth-service" &
"$BUILD_DIR/storage-writer" &
"$BUILD_DIR/stream-processor" &
"$BUILD_DIR/ingest-gateway" &
"$BUILD_DIR/query-api" &

echo "Waiting for services to become ready..."
sleep 5
echo "Services started."
