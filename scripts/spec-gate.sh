#!/usr/bin/env bash
# Specification gate for the 2.0 line. Mandatory for every change, including spec- and docs-only
# changes that are exempt from the rest of scripts/local-ci.sh. Also run by local-ci.sh.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if command -v uv >/dev/null 2>&1; then
  SPEC=(uv run --project "$REPO_ROOT/models" system-spec)
  PY=(uv run --project "$REPO_ROOT/models" python)
elif command -v system-spec >/dev/null 2>&1; then
  SPEC=(system-spec)
  PY=(python3)
else
  echo "spec-gate: neither uv nor system-spec is installed" >&2
  exit 1
fi

failed=0
run() {
  local name="$1"; shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name" >&2; failed=1; fi
}

# The stage runs separately because the CLI skips stage invariants when --check-artifacts/--check-drift pass.
run "metamodel verification stage" "${SPEC[@]}" spec/system.yaml --stage verification
run "metamodel artifacts and drift" "${SPEC[@]}" spec/system.yaml --stage verification --check-artifacts --check-drift --root .
run "business baseline integrity" "${PY[@]}" scripts/check-business-baseline.py
run "UX contracts" "${PY[@]}" scripts/check-ux-contracts.py
run "readiness evidence policy" "${PY[@]}" scripts/spec-readiness.py --priority P0 --strict
run "readiness policy regression tests" "${PY[@]}" scripts/tests/test_spec_readiness.py

exit "$failed"
