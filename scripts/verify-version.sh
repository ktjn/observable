#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="$(tr -d '[:space:]' < VERSION)"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
  echo "VERSION must contain a valid semantic version, got: '$version'" >&2
  exit 1
fi

failed=0

check_equals() {
  local label="$1"
  local actual="$2"
  if [[ "$actual" != "$version" ]]; then
    echo "$label is '$actual', expected '$version' from VERSION" >&2
    failed=1
  fi
}

while IFS= read -r manifest; do
  package_version="$(awk '
    /^\[package\]$/ { in_package=1; next }
    /^\[/ { in_package=0 }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$manifest")"

  if [[ -z "$package_version" ]]; then
    echo "$manifest has no explicit [package] version" >&2
    failed=1
  else
    check_equals "$manifest package version" "$package_version"
  fi
done < <(find libs services -mindepth 2 -maxdepth 2 -name Cargo.toml -print | sort)

chart_version="$(awk '$1 == "version:" { print $2; exit }' charts/observable/Chart.yaml | tr -d '"')"
chart_app_version="$(awk '$1 == "appVersion:" { print $2; exit }' charts/observable/Chart.yaml | tr -d '"')"
common_chart_version="$(awk '$1 == "version:" { print $2; exit }' charts/observable-common/Chart.yaml | tr -d '"')"
common_dependency_version="$(awk '
  $1 == "-" && $2 == "name:" && $3 == "observable-common" { found=1; next }
  found && $1 == "version:" { gsub(/"/, "", $2); print $2; exit }
' charts/observable/Chart.yaml)"

check_equals "charts/observable/Chart.yaml version" "$chart_version"
check_equals "charts/observable/Chart.yaml appVersion" "$chart_app_version"
check_equals "charts/observable-common/Chart.yaml version" "$common_chart_version"
check_equals "observable-common dependency version" "$common_dependency_version"

if (( failed != 0 )); then
  exit 1
fi

echo "All product versions match VERSION ($version)."
