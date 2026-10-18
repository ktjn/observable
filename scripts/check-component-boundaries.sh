#!/usr/bin/env bash
set -euo pipefail

# check-component-boundaries.sh
#
# Lint cross-component source import boundaries for the 0.2 decomposition.
#
# Enforces the "share contracts, not implementation" rule from
# docs/component-decomposition.md §Cross-Boundary Rules and the ADR-035
# constraint "no new cross-component source dependency may be introduced".
#
# Usage:
#   bash scripts/check-component-boundaries.sh
#
# What it checks:
# - A file owned by one deployable component referencing another component's
#   unpublished source (Rust crate/module paths such as `auth_service::` or
#   `admin_service::`, and direct frontend path imports `apps/frontend/`).
#
# What it does not flag:
# - The shared `contracts` sources (`contracts/**`,
#   `libs/observable-storage-contracts/**`): consuming the published contracts
#   is the supported boundary.
# - `distribution/**`: the distribution component names every other component's
#   package path by design and contains no importable service source.
# - Files whose path is not mapped to a component (root config, `scripts/`,
#   `charts/`, `libs/observable-*` implementation utilities, tests fixtures).
#
# Known transitional couplings are recorded, with a required reason, in
# scripts/component-boundary-allowlist.txt. New couplings fail the lint.
#
# Runs standalone and as the "Component boundary lint" step of
# scripts/local-ci.sh. Exit status is non-zero when a violation is found.

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ALLOWLIST="$ROOT/scripts/component-boundary-allowlist.txt"

# Directories whose contents are build output, generated, vendored, or VCS data
# rather than component source. These are OR-ed into a single negated group
# below; a bare list of `-path` predicates would AND together.
EXCLUDES=(
  "*/target/*"
  "*/generated/*"
  "*/node_modules/*"
  "*/.git/*"
  "*/dist/*"
  "*/build/*"
  "*/__pycache__/*"
)

# Owner of a path, by longest matching component root. `find .` emits paths
# prefixed with "./", so strip that before matching the patterns below.
owner_of() {
  local f="${1#./}"
  case "$f" in
    contracts/*|libs/observable-storage-contracts/*) echo contracts ;;
    services/auth-service/*|libs/observable-auth/*)  echo auth ;;
    services/admin-service/*)                        echo control ;;
    services/ingest-gateway/*)                       echo ingest ;;
    services/stream-processor/*)                     echo process ;;
    services/storage-writer/*|migrations/clickhouse/*) echo store_clickhouse ;;
    services/query-api/*|libs/query-core/*)          echo query ;;
    services/alert-evaluator/*)                      echo alerting ;;
    apps/frontend/*)                                 echo web ;;
    distribution/*)                                  echo distribution ;;
    *) echo unknown ;;
  esac
}

# Source-reference patterns, keyed by the component whose source they name.
# `contracts` and `distribution` are intentionally absent: importing the shared
# contracts is supported, and distribution composes every component by design.
declare -A COMPONENT_SOURCE_REFS=(
  [auth]='\bauth_service::'
  [control]='\badmin_service::|\bcontrol_plane::'
  [ingest]='\bingest_gateway::'
  [process]='\bstream_processor::'
  [query]='\bquery_api::|\bquery_core::'
  [alerting]='\balert_evaluator::'
  [store_clickhouse]='\bstorage_writer::'
  [web]='apps/frontend/'
)

# True when <file> referencing <component> is a documented, accepted coupling.
allowed() {
  local file="${1#./}" target="$2" pattern pat_target reason
  [[ -f "$ALLOWLIST" ]] || return 1
  while read -r pattern pat_target reason; do
    [[ -z "$pattern" || "$pattern" == \#* ]] && continue
    if [[ -z "${reason:-}" ]]; then
      echo "invalid allowlist entry (reason required): $pattern" >&2
      exit 2
    fi
    if [[ "$file" == $pattern && "$target" == "$pat_target" ]]; then
      return 0
    fi
  done <"$ALLOWLIST"
  return 1
}

echo "Checking component boundaries..."
cd "$ROOT"

combined=""
for component in "${!COMPONENT_SOURCE_REFS[@]}"; do
  combined+="${combined:+|}"
  combined+="${COMPONENT_SOURCE_REFS[$component]}"
done

# `! \( -path A -o -path B -o ... \)`: the patterns must be OR-ed inside the
# group. AND-ing them (the original bug) makes the negated group always true,
# so nothing is excluded; appending `-o` to a bare list leaves a trailing
# operator and breaks `find`.
exclude_args=()
for pattern in "${EXCLUDES[@]}"; do
  [[ ${#exclude_args[@]} -gt 0 ]] && exclude_args+=(-o)
  exclude_args+=(-path "$pattern")
done

violations=()
accepted=0

# One grep pass over all source files; the per-line component lookup then runs
# only on matched lines. Scanning file-by-file with a grep per line instead
# takes minutes on this tree.
while IFS= read -r match; do
  [[ -z "$match" ]] && continue
  file="${match%%:*}"
  rest="${match#*:}"
  text="${rest#*:}"
  owner=$(owner_of "$file")
  case "$owner" in
    unknown|contracts|distribution) continue ;;
  esac
  # Skip comment lines: prose (doc-comments, YAML/TOML comments, SQL comments)
  # may name another component without creating a source dependency.
  [[ "$text" =~ ^[[:space:]]*(//|/\*|\*|#) ]] && continue
  for target in "${!COMPONENT_SOURCE_REFS[@]}"; do
    [[ "$target" == "$owner" ]] && continue
    grep -qE "${COMPONENT_SOURCE_REFS[$target]}" <<<"$text" || continue
    if allowed "$file" "$target"; then
      accepted=$((accepted + 1))
      continue
    fi
    violations+=("$file: $text")
  done
done < <(
  find . -type f \( \
    -name "*.rs" -o -name "*.ts" -o -name "*.tsx" -o -name "*.js" -o -name "*.jsx" \
    -o -name "*.py" -o -name "*.toml" -o -name "*.yaml" -o -name "*.yml" \) \
    ! \( "${exclude_args[@]}" \) \
    -exec grep -HnIE "$combined" {} + 2>/dev/null || true
)

if [[ ${#violations[@]} -eq 0 ]]; then
  echo "✓ No cross-component source import violations detected."
  if [[ $accepted -gt 0 ]]; then
    echo "  ($accepted documented transitional coupling(s) accepted via scripts/component-boundary-allowlist.txt)"
  fi
  exit 0
else
  echo "✗ Found ${#violations[@]} potential cross-component source import violations:"
  echo
  printf '  %s\n' "${violations[@]}" | head -100
  echo
  echo "See docs/component-decomposition.md §Cross-Boundary Rules."
  echo "If a coupling is accepted transitional work, record it with a reason in"
  echo "scripts/component-boundary-allowlist.txt instead of removing this check."
  exit 1
fi
