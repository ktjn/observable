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
# Usage:
#   bash scripts/run-formal-verification.sh
#
# Environment:
#   TLA2TOOLS_JAR  use an existing tla2tools.jar instead of the cache

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_DIR="$REPO_ROOT/formal/tla"

TLA_VERSION="v1.7.4"
TLA_SHA256="936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/${TLA_VERSION}/tla2tools.jar"

JAR="${TLA2TOOLS_JAR:-$REPO_ROOT/.tla/tla2tools.jar}"

command -v java >/dev/null 2>&1 \
  || { echo "FAIL: java not found on PATH (TLC needs a Java runtime)" >&2; exit 1; }

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

for cfg in "${configs[@]}"; do
  model="${cfg%.cfg}.tla"
  name="$(basename "${cfg%.cfg}")"
  [[ -f "$model" ]] || { echo "FAIL: no model for $cfg" >&2; exit 1; }

  echo "==> TLC ${name}"
  metadir="$(mktemp -d)"
  if ! ( cd "$MODEL_DIR" && java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC \
           -config "$(basename "$cfg")" -metadir "$metadir" -cleanup "$(basename "$model")" ); then
    rm -rf "$metadir"
    echo "FAIL: TLC reported a violation in $name (see the counterexample above)" >&2
    exit 1
  fi
  rm -rf "$metadir"
  echo "OK  ${name}"
done

echo "OK  all TLA+ models checked"
