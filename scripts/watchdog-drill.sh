#!/usr/bin/env bash
# OBS-29-06 watchdog drill (docs/assurance-relocations.md E4) against the running compose stack.
#
# The watchdog (scripts/watchdog.py) runs on the host, outside the compose project, with its own
# notification route (a receiver process on the host). The drill stops every container of the
# monitored stack, platform alerting included, and asserts that the watchdog detects the loss and
# notifies through its own route within its detection budget. It then restarts the stack and
# asserts recovery is reported. Figures: config/exercises/platform-exercises.json (.watchdog).
#
# Requires: docker compose stack up and healthy, python3, jq. Writes $DRILL_OUT/report.json.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
FIGURES="config/exercises/platform-exercises.json"
DRILL_OUT="${DRILL_OUT:-$REPO_ROOT/target/exercises/watchdog}"
SINK_PORT="${WATCHDOG_SINK_PORT:-9198}"

for c in docker python3 jq curl; do
  command -v "$c" >/dev/null || { echo "FAIL: '$c' is required"; exit 1; }
done

interval=$(jq -r '.watchdog.interval_seconds' "$FIGURES")
failures=$(jq -r '.watchdog.failures' "$FIGURES")
slack=$(jq -r '.watchdog.notify_slack_seconds' "$FIGURES")
budget=$(python3 -c "print(int($interval * $failures + $slack))")

# The monitored stack's essential detection and notification path.
targets=(
  "ingest-gateway=http://127.0.0.1:4318/health"
  "query-api=http://127.0.0.1:8090/health"
  "alert-evaluator=http://127.0.0.1:4322/health"
  "health-service=http://127.0.0.1:4326/readyz"
  "frontend=http://127.0.0.1:${FRONTEND_PORT:-5173}/"
)

rm -rf "$DRILL_OUT"
mkdir -p "$DRILL_OUT"
sink_file="$DRILL_OUT/notifications.jsonl"
pids=()
stack_stopped=0
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  if [[ $stack_stopped -eq 1 ]]; then docker compose start >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

for t in "${targets[@]}"; do
  curl -sf -o /dev/null "${t#*=}" || { echo "FAIL: ${t%%=*} is not healthy before the drill"; exit 1; }
done

python3 scripts/watchdog.py receive --port "$SINK_PORT" --out "$sink_file" >"$DRILL_OUT/receiver.log" 2>&1 &
pids+=("$!")
for _ in $(seq 1 50); do
  (exec 3<>"/dev/tcp/127.0.0.1/$SINK_PORT") 2>/dev/null && break
  sleep 0.2
done

watch_args=()
for t in "${targets[@]}"; do watch_args+=(--target "$t"); done
python3 scripts/watchdog.py watch "${watch_args[@]}" \
  --notify-url "http://127.0.0.1:$SINK_PORT/" \
  --interval "$interval" --failures "$failures" --observer "drill-$(hostname)" \
  >"$DRILL_OUT/watchdog.jsonl" 2>"$DRILL_OUT/watchdog.err" &
pids+=("$!")

# Healthy baseline: a full detection window with no notification.
sleep "$budget"
if [[ -s "$sink_file" ]]; then
  echo "FAIL: watchdog notified while the stack was healthy"; cat "$sink_file"; exit 1
fi

echo "Stopping the monitored stack (every compose container)..."
stopped_at=$(date +%s)
stack_stopped=1
docker compose stop >/dev/null

count_events() {
  [[ -f "$sink_file" ]] || { echo 0; return; }
  jq -s --arg e "$1" '[.[] | select(.payload.event == $e) | .payload.target] | unique | length' "$sink_file"
}

deadline=$((stopped_at + budget))
while (( $(date +%s) < deadline )) && (( $(count_events monitored_target_lost) < ${#targets[@]} )); do
  sleep 1
done
lost=$(count_events monitored_target_lost)
detected_after=$(( $(date +%s) - stopped_at ))

echo "Restarting the monitored stack..."
docker compose start >/dev/null
stack_stopped=0
recover_deadline=$(( $(date +%s) + 180 ))
while (( $(date +%s) < recover_deadline )) && (( $(count_events monitored_target_recovered) < ${#targets[@]} )); do
  sleep 2
done
recovered=$(count_events monitored_target_recovered)

passed=true
misses=()
if (( lost < ${#targets[@]} )); then
  passed=false; misses+=("loss notified for $lost of ${#targets[@]} targets within ${budget}s")
fi
if (( recovered < ${#targets[@]} )); then
  passed=false; misses+=("recovery notified for $recovered of ${#targets[@]} targets")
fi

jq -n --argjson passed "$passed" --argjson lost "$lost" --argjson recovered "$recovered" \
  --argjson targets "${#targets[@]}" --argjson budget "$budget" --argjson detected "$detected_after" \
  --arg misses "$(printf '%s\n' "${misses[@]:-}")" \
  '{requirement: "OBS-29-06", passed: $passed, targets: $targets, lost_notified: $lost,
    recovered_notified: $recovered, detection_budget_seconds: $budget,
    stop_to_last_loss_seconds: $detected, misses: ($misses | split("\n") | map(select(. != "")))}' \
  | tee "$DRILL_OUT/report.json"

[[ $passed == true ]]
