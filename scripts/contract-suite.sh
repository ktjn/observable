#!/usr/bin/env bash
# Shared real/mock contract suite runner for provider-owned mocks (COMPONENT-ISOLATION ISO-04).
# The metamodel isolation check runs it with SYSTEM_SPEC_MODE=real|mock and
# SYSTEM_SPEC_CAPABILITY=<capability>; it can also be run by hand:
#   SYSTEM_SPEC_MODE=mock SYSTEM_SPEC_CAPABILITY=decide-access bash scripts/contract-suite.sh
set -euo pipefail

mode="${SYSTEM_SPEC_MODE:-}"
capability="${SYSTEM_SPEC_CAPABILITY:-}"

case "$capability" in
  decide-access|lookup-principals)
    case "$mode" in
      mock) exec cargo test -p observable-auth --test contract_mock ;;
      real) exec cargo test -p auth-service --test authz_contract_integration ;;
    esac ;;
esac

echo "contract-suite: unsupported mode '$mode' or capability '$capability'" >&2
exit 2
