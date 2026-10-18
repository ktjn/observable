#!/usr/bin/env bash
set -euo pipefail

# check-component-boundaries.sh
#
# Lint cross-component source import boundaries for the 0.2 decomposition.
#
# Purpose:
# - Prevent accidental imports of another deployable component's unpublished source.
# - Enforce "no cross-owner SQL/imports" as specified in docs/component-decomposition.md.
#
# Usage:
#   bash scripts/check-component-boundaries.sh
#
# What it checks:
# - For each deployable component's source roots, flags imports/usages that reference
#   another component's source root (Rust crate paths like `services/control_plane`,
#   `crate::` cross-crate references to sibling component source, or path-based imports).
# - Excludes: target/, generated/, node_modules/, .git/, dist/, build/, __pycache__.
# - Pragmatic: flags likely cross-component source dependencies; internal client calls
#   via published contracts are allowed and should not be flagged.
#
# This is a developer lint (run locally). It is not wired into local-ci.sh in this slice.
# Exit non-zero if violations found.

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# Component source roots (monorepo paths)
# Define what constitutes "component source" that must not be imported by other components.
declare -A COMPONENT_ROOTS=(
  ["contracts"]="contracts"
  ["auth"]="services/auth-service libs/observable-auth"
  ["control"]="services/admin-service"
  ["ingest"]="services/ingest-gateway services/processor services/stream-processor"
  ["process"]="services/stream-processor"
  ["store_clickhouse"]="services/storage-writer libs/observable-storage-contracts migrations/clickhouse"
  ["query"]="services/query-api libs/query-core"
  ["alerting"]="services/alert-evaluator"
  ["web"]="apps/frontend"
  ["distribution"]="distribution"
)

# Map each file to its owning component(s) by path prefix
# For simplicity, a file belongs to the component whose root is the longest matching prefix.
# Build inverse mapping by scanning files is expensive; instead we check: for a given file F
# owned by C, look for references to any D != C's roots.

# Find all source files to consider
# Include: Rust, TS/TSX, JS/JSX, Python (if any), proto-like references in source
EXCLUDES=(
  -path "*/target/*"
  -path "*/generated/*"
  -path "*/node_modules/*"
  -path "*/.git/*"
  -path "*/dist/*"
  -path "*/build/*"
  -path "*/__pycache__/*"
)

VIOLATIONS=()
VIOLATION_COUNT=0

# Helper: get owner component for a file path
get_owner() {
  local f="$1"
  # Try specific owners in order of specificity (longest prefix first)
  # control before ingest/process overlap? admin-service is control; stream-processor is both ingest/process roots but we assign by primary owner
  # Define primary owners with precedence
  case "$f" in
    contracts/*|libs/observable-storage-contracts/*)
      echo "contracts"; return 0 ;;
    services/auth-service/*|libs/observable-auth/*)
      echo "auth"; return 0 ;;
    services/admin-service/*)
      echo "control"; return 0 ;;
    services/ingest-gateway/*)
      echo "ingest"; return 0 ;;
    services/processor/*)
      echo "ingest"; return 0 ;;
    services/stream-processor/*)
      echo "process"; return 0 ;;  # primary owner is process pipeline
    services/storage-writer/*|migrations/clickhouse/*)
      echo "store_clickhouse"; return 0 ;;
    services/query-api/*|libs/query-core/*)
      echo "query"; return 0 ;;
    services/alert-evaluator/*)
      echo "alerting"; return 0 ;;
    apps/frontend/*)
      echo "web"; return 0 ;;
    distribution/*)
      echo "distribution"; return 0 ;;
    *)
      echo "unknown"; return 0 ;;
  esac
}

# Check a single file for cross-component source imports
check_file() {
  local f="$1"
  local owner=$(get_owner "$f")
  [[ "$owner" == "unknown" ]] && return 0

  # Look for Rust crate paths that reference other component source crates
  # Common patterns:
  # - use services::control_plane::...
  # - use services::ingest_gateway::...
  # - use services::processor::...
  # - use services::query_api::...
  # - use services::alert_evaluator::...
  # - use services::admin_service::...
  # - use services::auth_service::...
  # - use services::storage_writer::...
  # - use libs::observable_auth::...
  # - use libs::query_core::...
  # - use apps_frontend references in backend? unlikely; check others
  # Also path-based includes in TS? less common

  # Rust patterns
  while IFS= read -r line; do
    # Skip comments
    [[ "$line" =~ ^[[:space:]]*// ]] && continue
    [[ "$line" =~ ^[[:space:]]*#\[\|//\* ]] && continue

    # Check for cross-component crate references
    # control -> others must not import control source
    if [[ "$owner" != "control" ]]; then
      if grep -qE '\bservices::admin_service\b|\bservices::control_plane\b|\badmin_service::' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "ingest" ]]; then
      if grep -qE '\bservices::ingest_gateway\b|\bingest_gateway::|\bservices::processor\b|\bprocessor::' <<<"$line"; then
        # Allow if referencing process/ingest split intentionally? But ingest owns gateway+processor in this mapping
        # Also allow process to reference ingest? no, ingest consumed by process via queue
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "process" ]]; then
      if grep -qE '\bservices::stream_processor\b|\bstream_processor::' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "query" ]]; then
      if grep -qE '\bservices::query_api\b|\bquery_api::|\blibs::query_core\b' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "alerting" ]]; then
      if grep -qE '\bservices::alert_evaluator\b|\balert_evaluator::' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "auth" ]]; then
      if grep -qE '\bservices::auth_service\b|\bauth_service::|\blibs::observable_auth\b' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "store_clickhouse" ]]; then
      if grep -qE '\bservices::storage_writer\b|\bstorage_writer::|\blibs::observable_storage_contracts\b' <<<"$line"; then
        # Note: contracts crate may be shared intentionally via published contracts; this flags source-level references
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "web" ]]; then
      if grep -qE '\bapps/frontend\b|apps_frontend' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
    if [[ "$owner" != "distribution" ]]; then
      if grep -qE '\bdistribution::\b|distribution/' <<<"$line"; then
        VIOLATIONS+=("$f: $line")
        ((VIOLATION_COUNT++))
      fi
    fi
  done < "$f"
}

echo "Checking component boundaries..."
cd "$ROOT"

# Find all source files
while IFS= read -r -d '' f; do
  check_file "$f"
done < <(find . -type f \( -name "*.rs" -o -name "*.ts" -o -name "*.tsx" -o -name "*.js" -o -name "*.jsx" -o -name "*.py" -o -name "*.toml" -o -name "*.yaml" -o -name "*.yml" \) \
  "${EXCLUDES[@]}" -print0)

if [[ $VIOLATION_COUNT -eq 0 ]]; then
  echo "✓ No cross-component source import violations detected."
  exit 0
else
  echo "✗ Found $VIOLATION_COUNT potential cross-component source import violations:"
  echo
  printf '%s\n' "${VIOLATIONS[@]}" | head -100
  echo
  echo "See docs/component-decomposition.md §Cross-Boundary Rules."
  exit 1
fi
