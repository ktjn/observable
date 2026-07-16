#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

version="$(tr -d '[:space:]' < VERSION)"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
  echo "VERSION must contain a semantic version, got: '$version'" >&2
  exit 1
fi

fail=0

check_value() {
  local label="$1"
  local actual="$2"
  if [[ "$actual" != "$version" ]]; then
    echo "$label is '$actual', expected '$version' from VERSION" >&2
    fail=1
  fi
}

workspace_version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' Cargo.toml)"
check_value "Cargo workspace version" "$workspace_version"

while IFS= read -r manifest; do
  package_version="$(sed -n '/^\[package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$manifest")"
  inherited="$(sed -n '/^\[package\]/,/^\[/s/^version\.workspace[[:space:]]*=[[:space:]]*\(true\)/\1/p' "$manifest")"
  if [[ "$inherited" != "true" ]]; then
    check_value "$manifest package version" "$package_version"
  fi
done < <(find libs services -name Cargo.toml -print | sort)

frontend_version="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' apps/frontend/package.json | head -n1)"
check_value "Frontend package version" "$frontend_version"

chart_version="$(sed -n 's/^version:[[:space:]]*"\{0,1\}\([^"[:space:]]*\)"\{0,1\}$/\1/p' charts/observable/Chart.yaml | head -n1)"
chart_app_version="$(sed -n 's/^appVersion:[[:space:]]*"\{0,1\}\([^"[:space:]]*\)"\{0,1\}$/\1/p' charts/observable/Chart.yaml | head -n1)"
common_chart_version="$(sed -n 's/^version:[[:space:]]*"\{0,1\}\([^"[:space:]]*\)"\{0,1\}$/\1/p' charts/observable-common/Chart.yaml | head -n1)"
check_value "Observable Helm chart version" "$chart_version"
check_value "Observable Helm appVersion" "$chart_app_version"
check_value "Observable common chart version" "$common_chart_version"

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi

echo "Version metadata matches VERSION ($version)"
