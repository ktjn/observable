#!/usr/bin/env bash
# Long-run platform exercises against the running compose stack (docs/assurance-relocations.md 4.8):
# OBS-19-08 load, OBS-19-09 growth, OBS-29-06 watchdog drill. Not part of every gate run:
# `bash scripts/local-ci.sh --containers --exercises`, or directly once `docker compose up -d --wait`.
# Figures: config/exercises/platform-exercises.json. Reports: target/exercises/.
#
#   bash scripts/platform-exercises.sh [load] [growth] [watchdog]   # default: all three
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
OUT="${EXERCISE_OUT:-$REPO_ROOT/target/exercises}"
mkdir -p "$OUT"
export EXERCISE_OUT="$OUT" EXERCISE_UID="$(id -u)" EXERCISE_GID="$(id -g)"
export EXERCISE_SINK_FILE="$OUT/notifications.jsonl"

selected=("$@")
[[ ${#selected[@]} -gt 0 ]] || selected=(load growth watchdog)
failed=()

run() {
  local name="$1"; shift
  echo "=== $name ==="
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; failed+=("$name"); fi
}

for e in "${selected[@]}"; do
  case "$e" in
    load) run "OBS-19-08 load" python3 scripts/platform-exercise.py load --report "$OUT/load.json" ;;
    growth)
      docker compose --profile exercises up -d --wait exercise-sink
      run "OBS-19-09 growth" python3 scripts/platform-exercise.py growth --report "$OUT/growth.json"
      docker compose --profile exercises stop exercise-sink >/dev/null
      ;;
    # Stops and restarts the whole stack: keep it last.
    watchdog) run "OBS-29-06 watchdog drill" env DRILL_OUT="$OUT/watchdog" bash scripts/watchdog-drill.sh ;;
    *) echo "unknown exercise: $e"; exit 2 ;;
  esac
done

[[ ${#failed[@]} -eq 0 ]] || { echo "FAIL: ${failed[*]}"; exit 1; }
