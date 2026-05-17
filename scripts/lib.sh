#!/usr/bin/env bash
# Shared helpers for kind-test.sh and testbench.sh. Source this file; do not execute directly.

log()  { echo ""; echo "==> [$(date +%H:%M:%S)] $*"; }
info() { echo "    $*"; }

# show_pods <namespace>
show_pods() {
  local ns="$1"
  echo ""
  kubectl get pods --namespace "$ns" -o wide 2>/dev/null || true
}

# dump_pod_events <namespace>
dump_pod_events() {
  local ns="$1"
  echo ""
  info "--- Pod status (namespace: $ns) ---"
  kubectl get pods --namespace "$ns" -o wide 2>/dev/null || true
  echo ""
  info "--- Recent events ---"
  kubectl get events --namespace "$ns" --sort-by='.lastTimestamp' 2>/dev/null | tail -20 || true
  echo ""
  info "--- Non-running pods ---"
  kubectl get pods --namespace "$ns" \
    --field-selector='status.phase!=Running' -o wide 2>/dev/null || true
}

# deployment_ready_now <resource> <namespace>
# NOTE: DaemonSets lack spec.replicas; this may give false positives for them. Use only for Deployments.
deployment_ready_now() {
  local resource="$1" ns="$2"
  local json
  json="$(kubectl get "$resource" --namespace "$ns" -o json 2>/dev/null)" || return 1

  local generation observed replicas updated ready available unavailable
  generation="$(jq -r  '.metadata.generation // 0'        <<<"$json")"
  observed="$(jq -r    '.status.observedGeneration // 0'   <<<"$json")"
  replicas="$(jq -r    '.spec.replicas // 1'               <<<"$json")"
  updated="$(jq -r     '.status.updatedReplicas // 0'      <<<"$json")"
  ready="$(jq -r       '.status.readyReplicas // 0'        <<<"$json")"
  available="$(jq -r   '.status.availableReplicas // 0'    <<<"$json")"
  unavailable="$(jq -r '.status.unavailableReplicas // 0'  <<<"$json")"

  [[ "$observed"    == "$generation" ]] \
    && [[ "$updated"     == "$replicas"   ]] \
    && [[ "$ready"       == "$replicas"   ]] \
    && [[ "$available"   == "$replicas"   ]] \
    && [[ "$unavailable" == "0"           ]]
}

# wait_for_rollout <resource> <namespace> [timeout]
# Returns non-zero on failure; does NOT call exit — callers own that decision.
wait_for_rollout() {
  local resource="$1" ns="$2" timeout="${3:-180s}"
  if deployment_ready_now "$resource" "$ns"; then
    info "$resource already ready at current generation"
    return 0
  fi
  info "waiting for $resource in ns=$ns (timeout: $timeout)"
  kubectl rollout status "$resource" --namespace "$ns" --timeout "$timeout"
}

# wait_for_rollouts_parallel <namespace> <timeout> <resource> [<resource> ...]
wait_for_rollouts_parallel() {
  local ns="$1" timeout="$2"
  shift 2
  local resources=("$@")

  local pids=() names=()
  for resource in "${resources[@]}"; do
    wait_for_rollout "$resource" "$ns" "$timeout" &
    pids+=($!)
    names+=("$resource")
  done

  local failed=0 i
  for i in "${!pids[@]}"; do
    if ! wait "${pids[$i]}"; then
      info "FAILED: ${names[$i]} did not become ready"
      failed=1
    fi
  done

  if [[ "$failed" -ne 0 ]]; then
    dump_pod_events "$ns"
    return 1
  fi
}

# build_images_parallel <"tag:variant:context" ...>
# Entry format matches TESTBENCH_IMAGES array: "image-tag:image-variant:build-context"
build_images_parallel() {
  local pids=() tags=()
  for entry in "$@"; do
    local tag="${entry%:*}" context="${entry##*:}"
    info "Building $tag from $context"
    docker build --tag "$tag" "$context" &
    pids+=($!)
    tags+=("$tag")
  done

  local i
  for i in "${!pids[@]}"; do
    wait "${pids[$i]}" || { echo "ERROR: docker build failed for ${tags[$i]}" >&2; exit 1; }
    info "${tags[$i]} built"
  done
}

# load_images_parallel <cluster-name> <image> [<image> ...]
load_images_parallel() {
  local cluster="$1"
  shift
  local pids=() imgs=()
  for img in "$@"; do
    info "Loading $img into kind cluster '$cluster'"
    kind load docker-image "$img" --name "$cluster" &
    pids+=($!)
    imgs+=("$img")
  done

  local i
  for i in "${!imgs[@]}"; do
    wait "${pids[$i]}" || { echo "ERROR: failed to load ${imgs[$i]}" >&2; exit 1; }
    info "${imgs[$i]} loaded"
  done
}
