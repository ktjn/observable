#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

version="$(tr -d '[:space:]' < VERSION)"
semver_re='^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$'

if [[ ! "$version" =~ $semver_re ]]; then
  echo "VERSION must contain a semantic version, got: $version" >&2
  exit 1
fi

failed=0

check_value() {
  local file="$1"
  local actual="$2"
  local field="$3"

  if [[ "$actual" != "$version" ]]; then
    echo "$file: $field is '$actual', expected '$version' from VERSION" >&2
    failed=1
  fi
}

while IFS= read -r manifest; do
  package_version="$(sed -n '/^\[package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$manifest" | head -n1)"
  if [[ -z "$package_version" ]]; then
    echo "$manifest: missing literal package version" >&2
    failed=1
  else
    check_value "$manifest" "$package_version" "package.version"
  fi
done < <(find libs services -name Cargo.toml -type f | sort)

frontend_version="$(python3 -c 'import json; print(json.load(open("apps/frontend/package.json"))["version"])')"
check_value "apps/frontend/package.json" "$frontend_version" "version"

while IFS= read -r chart; do
  chart_version="$(sed -n 's/^version:[[:space:]]*["'"']\?\([^"'"'[:space:]]*\)["'"']\?[[:space:]]*$/\1/p' "$chart" | head -n1)"
  if [[ -z "$chart_version" ]]; then
    echo "$chart: missing chart version" >&2
    failed=1
  else
    check_value "$chart" "$chart_version" "version"
  fi
done < <(find charts -mindepth 2 -maxdepth 2 -name Chart.yaml -type f | sort)

app_version="$(sed -n 's/^appVersion:[[:space:]]*["'"']\?\([^"'"'[:space:]]*\)["'"']\?[[:space:]]*$/\1/p' charts/observable/Chart.yaml | head -n1)"
check_value "charts/observable/Chart.yaml" "$app_version" "appVersion"

if (( failed )); then
  exit 1
fi

echo "All product version metadata matches VERSION=$version"
