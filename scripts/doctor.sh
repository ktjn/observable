#!/usr/bin/env bash
# scripts/doctor.sh
# Diagnostic and troubleshooting tool for self-hosted evaluation deployments.
#
# Inspects host prerequisites, port availability, container health, and
# service endpoints to provide actionable guidance for self-hosters.
#
# Usage:
#   bash scripts/doctor.sh [--preflight] [--verbose] [--timeout SECONDS]
#
# Options:
#   --preflight   Check host prerequisites and port availability before starting stack.
#   --verbose     Print detailed endpoint responses and container status.
#   --timeout N   Per-endpoint probe timeout in seconds (default: 3).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PREFLIGHT=false
VERBOSE=false
TIMEOUT=3

while [[ $# -gt 0 ]]; do
  case "$1" in
    --preflight)
      PREFLIGHT=true
      shift
      ;;
    --verbose)
      VERBOSE=true
      shift
      ;;
    --timeout)
      if [[ $# -lt 2 ]]; then
        echo "FAIL: --timeout requires a value" >&2
        exit 1
      fi
      TIMEOUT="$2"
      shift 2
      ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/doctor.sh [OPTIONS]

Options:
  --preflight   Check host prerequisites and port availability before starting.
  --verbose     Print detailed status and error logs.
  --timeout N   Probe timeout in seconds (default: 3).
  -h, --help    Show this message.
EOF
      exit 0
      ;;
    *)
      echo "FAIL: unknown argument '$1'" >&2
      exit 1
      ;;
  esac
done

ERRORS=0
WARNINGS=0

log_pass() {
  echo "  [OK]   $1"
}

log_warn() {
  echo "  [WARN] $1"
  WARNINGS=$((WARNINGS + 1))
}

log_fail() {
  echo "  [FAIL] $1"
  ERRORS=$((ERRORS + 1))
}

echo "=== Observable System Doctor ==="
echo ""

# 1. Host Tooling Prerequisites
echo "==> 1. Checking Tooling Prerequisites"
if command -v docker >/dev/null 2>&1; then
  docker_version="$(docker --version 2>/dev/null | awk '{print $3}' | tr -d ',' || echo 'unknown')"
  log_pass "Docker client: $docker_version"
  if docker info >/dev/null 2>&1; then
    log_pass "Docker daemon is running"
  else
    log_warn "Docker daemon is not responding (ensure Docker Desktop or dockerd is running)"
  fi
else
  log_fail "Docker is not installed or not in PATH. Please install Docker 20.10+."
fi

if docker compose version >/dev/null 2>&1; then
  compose_version="$(docker compose version --short 2>/dev/null || echo 'v2')"
  log_pass "Docker Compose: $compose_version"
else
  log_fail "Docker Compose v2 is required ('docker compose' plugin not found)."
fi

if command -v curl >/dev/null 2>&1; then
  log_pass "curl is available"
else
  log_fail "curl is required for health probe checks."
fi

# 2. Host Resource Checks
echo ""
echo "==> 2. Checking Host Resources"
if command -v free >/dev/null 2>&1; then
  total_mem_kb="$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || true)"
  if [[ -n "$total_mem_kb" && "$total_mem_kb" =~ ^[0-9]+$ ]]; then
    total_mem_gb="$(( total_mem_kb / 1024 / 1024 ))"
    if [[ "$total_mem_gb" -ge 8 ]]; then
      log_pass "Memory: ${total_mem_gb} GB total (meets recommended 8+ GB)"
    elif [[ "$total_mem_gb" -ge 4 ]]; then
      log_warn "Memory: ${total_mem_gb} GB total (meets 4 GB minimum; 8+ GB recommended for full stack)"
    else
      log_fail "Memory: ${total_mem_gb} GB total is below the 4 GB minimum required for evaluation."
    fi
  else
    log_pass "Memory check: free available"
  fi
else
  log_pass "Host memory: prerequisites assume 8 GB (skipped direct /proc read on this platform)"
fi

# 3. Port Conflict Preflight Check
PORTS_TO_CHECK=(
  "5173:Frontend UI"
  "4317:OTLP gRPC Ingest"
  "4318:OTLP HTTP Ingest"
  "4319:Auth Service"
  "4320:Storage Writer"
  "4321:Ingest Gateway Platform"
  "4322:Alert Evaluator"
  "4323:Stream Processor"
  "4324:Admin Service"
  "8082:Zitadel IdP"
  "8083:OpenFGA Authorization"
  "8090:Query API"
  "8123:ClickHouse HTTP"
  "9092:Redpanda Kafka Broker"
)

if [[ "$PREFLIGHT" == "true" ]]; then
  echo ""
  echo "==> 3. Preflight Port Availability Check"

  # Fast multi-port check via Python if available to avoid TCP timeout delays on Windows/macOS/Linux
  raw_ports=()
  for item in "${PORTS_TO_CHECK[@]}"; do
    raw_ports+=("${item%%:*}")
  done

  used_ports_str=""
  python_bin=""
  if command -v python3 >/dev/null 2>&1; then
    python_bin="python3"
  elif command -v python >/dev/null 2>&1; then
    python_bin="python"
  fi

  if [[ -n "$python_bin" ]]; then
    used_ports_str="$("$python_bin" -c "
import socket, sys
ports = sys.argv[1:]
used = []
for p in ports:
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(0.05)
        if s.connect_ex(('127.0.0.1', int(p))) == 0:
            used.append(p)
        s.close()
    except Exception:
        pass
print(' '.join(used))
" "${raw_ports[@]}" 2>/dev/null || true)"
  fi

  for item in "${PORTS_TO_CHECK[@]}"; do
    port="${item%%:*}"
    desc="${item#*:}"
    port_in_use=false
    if [[ -n "$python_bin" ]]; then
      if [[ " $used_ports_str " =~ [[:space:]]${port}[[:space:]] ]]; then
        port_in_use=true
      fi
    else
      if (echo > /dev/tcp/127.0.0.1/"$port") >/dev/null 2>&1; then
        port_in_use=true
      fi
    fi

    if [[ "$port_in_use" == "true" ]]; then
      log_warn "Port $port ($desc) is already in use by another process."
      if [[ "$port" == "8083" || "$port" == "8080" ]]; then
        echo "         Hint: If kind testbench is running, delete it with: kind delete cluster --name observable-test"
      fi
    else
      log_pass "Port $port ($desc) is free"
    fi
  done

  echo ""
  echo "=== Preflight Summary ==="
  if [[ $ERRORS -eq 0 && $WARNINGS -eq 0 ]]; then
    echo "Ready to start stack: run 'docker compose up -d' or 'make dev'."
    exit 0
  elif [[ $ERRORS -eq 0 ]]; then
    echo "Preflight completed with $WARNINGS warning(s)."
    exit 0
  else
    echo "Preflight failed with $ERRORS error(s)."
    exit 1
  fi
fi

# 4. Container Health Status
echo ""
echo "==> 3. Inspecting Docker Compose Containers"
if ! docker compose ps >/dev/null 2>&1; then
  log_fail "Cannot query Docker Compose status. Is Docker daemon running and stack started?"
else
  containers="$(docker compose ps --format '{{.Service}}\t{{.State}}\t{{.Health}}' 2>/dev/null || true)"
  if [[ -z "$containers" ]]; then
    log_warn "No running containers found. Start the evaluation stack with 'docker compose up -d --build'."
  else
    while IFS=$'\t' read -r svc state health; do
      if [[ -z "$svc" ]]; then continue; fi
      if [[ "$health" == "healthy" ]]; then
        log_pass "$svc: running ($health)"
      elif [[ "$state" == "running" && -z "$health" ]]; then
        log_pass "$svc: running"
      else
        log_fail "$svc: state='$state', health='${health:-none}'"
        if [[ "$VERBOSE" == "true" ]]; then
          echo "         Last log lines for $svc:"
          docker compose logs --tail 10 "$svc" 2>&1 | sed 's/^/           /' || true
        fi
      fi
    done <<< "$containers"
  fi
fi

# 5. Service Endpoint Connectivity Probes
echo ""
echo "==> 4. Probing Service Health Endpoints"
ENDPOINTS=(
  "http://localhost:5173|Frontend Web UI (5173)"
  "http://localhost:4321/health|Ingest Gateway Health (4321)"
  "http://localhost:8090/health|Query API Health (8090)"
  "http://localhost:4319/health|Auth Service Health (4319)"
  "http://localhost:4324/health|Admin Service Health (4324)"
  "http://localhost:4322/health|Alert Evaluator Health (4322)"
  "http://localhost:4320/health|Storage Writer Health (4320)"
  "http://localhost:4323/health|Stream Processor Health (4323)"
  "http://localhost:8123/ping|ClickHouse Ping (8123)"
  "http://localhost:8083/healthz|OpenFGA Healthz (8083)"
  "http://localhost:8082/debug/ready|Zitadel Ready (8082)"
)

for item in "${ENDPOINTS[@]}"; do
  url="${item%%|*}"
  label="${item#*|}"
  if curl -sf --max-time "$TIMEOUT" "$url" >/dev/null 2>&1; then
    log_pass "$label is responding"
  else
    log_warn "$label did not respond at $url"
  fi
done

echo ""
echo "=== Doctor Diagnosis Summary ==="
if [[ $ERRORS -eq 0 && $WARNINGS -eq 0 ]]; then
  echo "Status: ALL SYSTEMS HEALTHY. Platform is ready for evaluation."
  exit 0
elif [[ $ERRORS -eq 0 ]]; then
  echo "Status: OPERATIONAL WITH WARNINGS ($WARNINGS warning(s))."
  echo "Tip: Run with --verbose for detailed container logs and diagnostics."
  exit 0
else
  echo "Status: DEGRADED ($ERRORS error(s), $WARNINGS warning(s))."
  echo "Troubleshooting Tips:"
  echo "  1. View failing container logs: docker compose logs <service-name>"
  echo "  2. Reset volumes and restart clean: make reset-volumes && make dev"
  echo "  3. Check port conflicts: bash scripts/doctor.sh --preflight"
  exit 1
fi
