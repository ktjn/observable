#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

version="$(tr -d '[:space:]' < VERSION)"
semver='^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$'

if [[ ! "$version" =~ $semver ]]; then
  echo "VERSION must contain a semantic version, got: $version" >&2
  exit 1
fi

failures=0

check_value() {
  local file="$1"
  local label="$2"
  local actual="$3"

  if [[ "$actual" != "$version" ]]; then
    echo "$file: $label is '$actual', expected '$version' from VERSION" >&2
    failures=1
  fi
}

while IFS= read -r manifest; do
  package_version="$(awk '
    /^\[package\]$/ { in_package=1; next }
    /^\[/ { if (in_package) exit }
    in_package && /^version[[:space:]]*=/ {
      value=$0
      sub(/^[^=]*=[[:space:]]*"/, "", value)
      sub(/".*$/, "", value)
      print value
      exit
    }
  ' "$manifest")"
  if [[ -n "$package_version" ]]; then
    check_value "$manifest" "package.version" "$package_version"
  fi
done < <(find libs services -name Cargo.toml -type f | sort)

frontend_version="$(node -p "require('./apps/frontend/package.json').version")"
check_value "apps/frontend/package.json" "version" "$frontend_version"

chart_version="$(awk '/^version:/ { print $2; exit }' charts/observable/Chart.yaml | tr -d '"')"
check_value "charts/observable/Chart.yaml" "version" "$chart_version"

chart_app_version="$(awk '/^appVersion:/ { print $2; exit }' charts/observable/Chart.yaml | tr -d '"')"
check_value "charts/observable/Chart.yaml" "appVersion" "$chart_app_version"

if [[ -n "${GITHUB_REF_NAME:-}" && "${GITHUB_REF_TYPE:-}" == "tag" ]]; then
  expected_tag="v$version"
  if [[ "$GITHUB_REF_NAME" != "$expected_tag" ]]; then
    echo "release tag is '$GITHUB_REF_NAME', expected '$expected_tag' from VERSION" >&2
    failures=1
  fi
fi

if (( failures != 0 )); then
  exit 1
fi

echo "Product version $version is synchronized."
