#!/bin/bash
set -euo pipefail

# scripts/ci.sh - Unified check and build script
# This script delegates all checks (fmt, clippy, test) to the Docker build process.
# This ensures a hermetic environment and avoids double-downloading crates.

echo "==> Running unified CI process (fmt, clippy, test, and image build)..."

# Use --load to ensure the image is available to the local Docker daemon
if command -v docker-buildx >/dev/null 2>&1 || docker help buildx >/dev/null 2>&1; then
  docker buildx build --load --tag observable-services:local .
else
  docker build --tag observable-services:local .
fi

echo "==> Success! Hermetic validation and image build complete."
