# Ingest Pipeline and Queue Backpressure Runbook

**Incident class:** ingest gateway failure, OTLP 429/503 responses, or Redpanda stream backpressure

This runbook guides operators through diagnosing and resolving ingestion degradations, authentication rejection spikes, and stream buffer saturation in the Kubernetes deployment.

## When to use this runbook

Use this runbook when:
- Clients or collectors report HTTP 503 (`Service Unavailable`) or HTTP 429 (`Too Many Requests`) from `ingest-gateway`.
- `ingest_requests_total{status="error"}` or rate-limiting counters spike in `/metrics`.
- Redpanda queue depth on `telemetry.raw.v1` grows continuously without consumer draining.
- Telemetry ingestion throughput falls below expected baseline while client traffic is steady.

## Architecture Context

```
OTLP Clients / Collectors
          |
          v (HTTP :4318 / gRPC :4317)
  [ingest-gateway]  <--->  [auth-service] (:4319 cache)
          |
          v (Kafka protocol)
    Redpanda: `telemetry.raw.v1`
          |
          v
  [stream-processor]  --->  Redpanda: `telemetry.normalized.v1`
```

1. `ingest-gateway` validates API keys against `auth-service` (with an in-memory cache).
2. Valid payloads are published to Redpanda topic `telemetry.raw.v1`.
3. If Redpanda is unavailable, `ingest-gateway` fails closed with HTTP 503 so clients retry.
4. If tenant cardinality limits or burst limits are exceeded, `ingest-gateway` rejects with HTTP 429.

## Immediate Triage

1. **Check ingest gateway pod health**:
   ```bash
   kubectl get pods -n observable -l app.kubernetes.io/name=ingest-gateway
   kubectl logs -n observable deploy/ingest-gateway --tail=100
   ```

2. **Probe live health and platform endpoints**:
   ```bash
   kubectl port-forward -n observable service/ingest-gateway 4321:4321 &
   PF_PID=$!
   curl -fsS http://localhost:4321/health
   curl -fsS http://localhost:4321/metrics | grep -E "ingest_requests|rate_limit|queue"
   kill $PF_PID
   ```

3. **Check Redpanda broker and topic health**:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk cluster info
   kubectl exec -n observable -it deploy/redpanda -- rpk topic describe telemetry.raw.v1
   kubectl exec -n observable -it deploy/redpanda -- rpk group describe stream-processor
   ```

4. **Verify auth-service validation path**:
   ```bash
   kubectl logs -n observable deploy/auth-service --tail=50
   ```

## Mitigation & Recovery Paths

### Scenario A: HTTP 503 — Redpanda Broker Unreachable or Disk Full

**Symptom**: `ingest-gateway` logs `broker transport error` or `failed to produce to telemetry.raw.v1`.

1. Verify Redpanda PVC storage utilization:
   ```bash
   kubectl get pvc -n observable -l app.kubernetes.io/name=redpanda
   kubectl exec -n observable -it deploy/redpanda -- df -h /var/lib/redpanda/data
   ```
2. If disk is exhausted, increase PVC capacity in values or trigger segment cleanup:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk topic alter-config telemetry.raw.v1 --set retention.ms=86400000
   ```
3. Restart stuck broker pods if metadata is stale:
   ```bash
   kubectl rollout restart -n observable statefulset/redpanda
   ```

### Scenario B: HTTP 429 — Cardinality Limit or Quota Rejection

**Symptom**: Client requests return 429; `ingest_rejected_cardinality_total` increments.

1. Inspect tenant metrics to identify high-cardinality emitters:
   ```bash
   kubectl logs -n observable deploy/ingest-gateway | grep "cardinality limit exceeded" | head -n 20
   ```
2. If genuine burst from a verified tenant, temporarily increase quota in `admin-service` or Helm values:
   ```bash
   curl -X PATCH http://admin-service:4324/v1/tenants/<tenant-id>/limits \
     -H 'Content-Type: application/json' \
     -H "Authorization: Bearer $INTERNAL_SERVICE_TOKEN" \
     -d '{"max_spans_per_sec": 10000}'
   ```

### Scenario C: Stream Processor Lagging on `telemetry.raw.v1`

**Symptom**: `rpk group describe stream-processor` shows high lag.

1. Scale up stream-processor replicas (up to the partition count of `telemetry.raw.v1`):
   ```bash
   kubectl scale -n observable deploy/stream-processor --replicas=4
   ```
2. Verify batch latency and memory in `stream-processor`:
   ```bash
   kubectl logs -n observable deploy/stream-processor --tail=50
   ```

## Restore and Verify

1. Send a test trace through the ingest endpoint:
   ```bash
   curl -X POST http://localhost:4318/v1/traces \
     -H 'Content-Type: application/json' \
     -H "Authorization: Bearer $API_KEY" \
     -d '{"resourceSpans":[]}'
   ```
2. Assert HTTP 200 response.
3. Confirm consumer lag on `telemetry.raw.v1` trends down to 0.

## Escalation Criteria

Escalate to platform engineering if:
- Redpanda partitions are in degraded or under-replicated state.
- Ingestion data corruption prevents batch deserialization.
- Scaling stream-processor does not reduce consumer lag.
