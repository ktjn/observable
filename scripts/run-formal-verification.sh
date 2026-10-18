#!/usr/bin/env bash
set -euo pipefail

# run-formal-verification.sh
#
# Runs TLC against every committed model in formal/tla/ (see
# spec/19-formal-verification.md and formal/tla/README.md). Development-time
# only: no Observable service depends on TLA+.
#
# The TLA+ tools are pinned by version and sha256 digest. The JAR is cached
# outside version control and its digest is verified on every run, so a
# corrupted or substituted download cannot be used silently.
#
# Each model is checked under a wall-clock budget (TLC_TIMEOUT seconds, default
# 300). TLC output for every model is written to .tla/<model>.log, so a
# counterexample trace survives a failed run for inspection.
#
# Usage:
#   bash scripts/run-formal-verification.sh
#
# Environment:
#   TLA2TOOLS_JAR  use an existing tla2tools.jar instead of the cache
#   TLC_TIMEOUT    wall-clock budget per model, in seconds (default 300)

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_DIR="$REPO_ROOT/formal/tla"

TLA_VERSION="v1.7.4"
TLA_SHA256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/${TLA_VERSION}/tla2tools.jar"

JAR="${TLA2TOOLS_JAR:-$REPO_ROOT/.tla/tla2tools.jar}"
TLC_TIMEOUT="${TLC_TIMEOUT:-300}"
LOG_DIR="$REPO_ROOT/.tla"

command -v java >/dev/null 2>&1 \
  || { echo "FAIL: java not found on PATH (TLC needs a Java runtime)" >&2; exit 1; }

# A wall-clock budget needs `timeout` (GNU coreutils; present in Git Bash/Linux).
# Fall back to an unbounded run with a warning rather than failing on macOS.
if command -v timeout >/dev/null 2>&1; then
  RUN_TIMEOUT=(timeout "$TLC_TIMEOUT")
else
  echo "WARN: 'timeout' not found; running TLC without a wall-clock budget" >&2
  RUN_TIMEOUT=()
fi

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "FAIL: no sha256 tool (sha256sum/shasum) on PATH" >&2
    exit 1
  fi
}

if [[ ! -f "$JAR" ]]; then
  echo "Downloading pinned TLA+ tools ${TLA_VERSION} to $JAR"
  mkdir -p "$(dirname "$JAR")"
  curl -fsSL -o "$JAR" "$TLA_URL" \
    || { echo "FAIL: could not download $TLA_URL" >&2; rm -f "$JAR"; exit 1; }
fi

actual="$(sha256_of "$JAR")"
[[ "$actual" == "$TLA_SHA256" ]] \
  || { echo "FAIL: $JAR sha256 $actual does not match pinned $TLA_SHA256" >&2; exit 1; }

shopt -s nullglob
configs=("$MODEL_DIR"/*.cfg)
[[ ${#configs[@]} -gt 0 ]] || { echo "FAIL: no TLC configurations in $MODEL_DIR" >&2; exit 1; }

mkdir -p "$LOG_DIR"
status=0
for cfg in "${configs[@]}"; do
  model="${cfg%.cfg}.tla"
  name="$(basename "${cfg%.cfg}")"
  [[ -f "$model" ]] || { echo "FAIL: no model for $cfg" >&2; exit 1; }

  echo "==> TLC ${name} (budget ${TLC_TIMEOUT}s)"
  metadir="$(mktemp -d)"
  log="$LOG_DIR/${name}.log"
  run_status=0
  ( cd "$MODEL_DIR" && "${RUN_TIMEOUT[@]}" java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC \
      -config "$(basename "$cfg")" -metadir "$metadir" -cleanup "$(basename "$model")" ) \
    >"$log" 2>&1 || run_status=$?
  rm -rf "$metadir"

  if [[ $run_status -ne 0 ]]; then
    if [[ $run_status -eq 124 ]]; then
      echo "FAIL: TLC exceeded the ${TLC_TIMEOUT}s budget for $name; log: $log" >&2
    else
      echo "FAIL: TLC reported a problem in $name (exit $run_status); log: $log" >&2
    fi
    # Surface the actionable counterexample trace from the preserved log.
    sed -n '/is violated/,/states generated/p' "$log" | head -60 >&2 || true
    status=1
    continue
  fi
  echo "OK  ${name} (log: $log)"
done

[[ $status -eq 0 ]] || { echo "FAIL: one or more TLA+ models failed" >&2; exit 1; }
echo "OK  all TLA+ models checked"
