# Kubernetes Failure Drills Runbook

**Incident / Drill class:** Validation of dependency interruption, fail-closed boundaries, and data recovery in Kubernetes environments.

This runbook guides operators through executing, observing, and validating disaster recovery and dependency-interruption drills on Kubernetes clusters. These procedures test whether the platform degrades gracefully, protects data durability, and recovers automatically without manual database patching.

---

## 1. Automated Execution via `kind-test.sh`

The fastest way to validate all dependency failure modes in a sandboxed, production-like topology is using the automated kind test harness with the `--with-drills` flag:

```bash
bash scripts/kind-test.sh --with-drills
```

### What `--with-drills` Executes:
1. Provisions a kind cluster with all infrastructure dependencies (ClickHouse, Redpanda, PostgreSQL, OpenFGA, Zitadel) and deploys the full Observable Helm chart.
2. Runs baseline health and ingest-to-query smoke tests.
3. **Drill 1 (Redpanda disruption):** Scales Redpanda to 0 replicas, asserts ingest-gateway fails closed with HTTP 503 (rejecting telemetry to prevent silent drops), restores Redpanda, and verifies automatic reconnection and pipeline delivery.
4. **Drill 2 (ClickHouse disruption):** Scales ClickHouse to 0 replicas, verifies query API fails, verifies ingest continues buffering incoming telemetry into Redpanda, restores ClickHouse, and confirms `storage-writer` drains the queued backlog into ClickHouse with full query visibility.
5. **Drill 3 (PostgreSQL disruption):** Suspends the CloudNativePG cluster, verifies that unauthenticated or non-cached token requests fail closed, restores PostgreSQL, and verifies automatic connection-pool recovery.
6. Exercises Helm upgrade and rollback to ensure configuration changes revert cleanly.

---

## 2. Manual Failure Drill Procedures

Use these step-by-step procedures to conduct controlled disaster recovery drills in staging or production Kubernetes clusters.

### Prerequisites

- `kubectl` configured with cluster administrator or platform operator credentials.
- Target namespace (default: `observable`).
- Test API token with ingestion permissions (`dev-api-key-0000` or production service token).
- Port-forward or external ingress access to `ingest-gateway` and `query-api`.

```bash
export NAMESPACE="observable"
export INGEST_URL="http://localhost:14318"
export QUERY_URL="http://localhost:18090"
export TOKEN="dev-api-key-0000"
export TENANT_ID="00000000-0000-0000-0000-000000000002"
```

---

### Drill 1: Redpanda Interruption & Fail-Closed Ingestion

**Objective:** Verify that when the message broker is unavailable, the ingest gateway fails closed immediately with HTTP 503 rather than buffering uncommitted data in memory or silently dropping requests.

#### Step 1: Baseline Ingest Verification
```bash
curl -sf -X POST "$INGEST_URL/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"drill-baseline"}}]},"scopeSpans":[{"spans":[{"traceId":"11111111111111111111111111111111","spanId":"2222222222222222","name":"baseline-span","startTimeUnixNano":"1000000000000000000","endTimeUnixNano":"1000000005000000000","status":{"code":1}}]}]}]}'
```
*Expected Result:* HTTP 200 OK.

#### Step 2: Simulate Broker Loss
Scale down the Redpanda StatefulSet:
```bash
kubectl scale statefulset redpanda --replicas=0 -n "$NAMESPACE"
kubectl wait --for=delete pod/redpanda-0 -n "$NAMESPACE" --timeout=60s
```

#### Step 3: Assert Fail-Closed Ingestion
Attempt to send a trace while Redpanda is down:
```bash
STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 -X POST "$INGEST_URL/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"drill-fail-closed"}}]},"scopeSpans":[{"spans":[{"traceId":"33333333333333333333333333333333","spanId":"4444444444444444","name":"fail-span","startTimeUnixNano":"1000000000000000000","endTimeUnixNano":"1000000005000000000","status":{"code":1}}]}]}]}')

echo "Ingest HTTP Status: $STATUS"
```
*Pass Criteria:* `STATUS` must be `>= 400` (typically 503 Service Unavailable).

#### Step 4: Restore Broker
```bash
kubectl scale statefulset redpanda --replicas=1 -n "$NAMESPACE"
kubectl wait --for=condition=ready pod/redpanda-0 -n "$NAMESPACE" --timeout=120s
```

#### Step 5: Verify Automatic Pipeline Recovery
Wait 10 seconds for producer pools to reconnect, then send a new trace:
```bash
TRACE_ID=$(openssl rand -hex 16)
curl -sf -X POST "$INGEST_URL/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"drill-recovered\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"5555555555555555\",\"name\":\"recovery-span\",\"startTimeUnixNano\":\"$(date +%s)000000000\",\"endTimeUnixNano\":\"$(date +%s)000000001\",\"status\":{\"code\":1}}]}]}]}"
```
Query `query-api` to confirm end-to-end receipt:
```bash
sleep 6
curl -sf -H "Authorization: Bearer $TOKEN" -H "X-Tenant-ID: $TENANT_ID" "$QUERY_URL/v1/traces/$TRACE_ID" | jq .
```
*Pass Criteria:* Trace details returned with HTTP 200.

---

### Drill 2: ClickHouse Storage Outage & Queue Buffering

**Objective:** Verify that during a storage outage, ingest remains operational by buffering telemetry into Redpanda, while query operations fail gracefully. Upon storage recovery, `storage-writer` drains the backlog without data loss.

#### Step 1: Simulate ClickHouse Outage
Scale down ClickHouse:
```bash
kubectl scale deployment clickhouse --replicas=0 -n "$NAMESPACE"
kubectl wait --for=delete pod -l app=clickhouse -n "$NAMESPACE" --timeout=60s
```

#### Step 2: Verify Query Fails Gracefully
```bash
QUERY_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
  -H "Authorization: Bearer $TOKEN" -H "X-Tenant-ID: $TENANT_ID" \
  "$QUERY_URL/v1/services")

echo "Query HTTP Status: $QUERY_STATUS"
```
*Pass Criteria:* HTTP 500 or 503.

#### Step 3: Verify Telemetry Buffering in Redpanda
Send a trace while ClickHouse is down:
```bash
BUFFERED_TRACE_ID=$(openssl rand -hex 16)
INGEST_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 -X POST "$INGEST_URL/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"drill-buffered\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$BUFFERED_TRACE_ID\",\"spanId\":\"6666666666666666\",\"name\":\"buffered-span\",\"startTimeUnixNano\":\"$(date +%s)000000000\",\"endTimeUnixNano\":\"$(date +%s)000000001\",\"status\":{\"code\":1}}]}]}]}")

echo "Buffered Ingest HTTP Status: $INGEST_STATUS"
```
*Pass Criteria:* HTTP 200 (data accepted and persisted to `telemetry.raw.v1` in Redpanda).

#### Step 4: Restore ClickHouse
```bash
kubectl scale deployment clickhouse --replicas=1 -n "$NAMESPACE"
kubectl wait deployment/clickhouse --for=condition=Available -n "$NAMESPACE" --timeout=120s
```

#### Step 5: Verify Backlog Drain and Data Integrity
Wait 10 seconds for `storage-writer` to flush buffered records:
```bash
sleep 10
RESULT=$(curl -sf -H "Authorization: Bearer $TOKEN" -H "X-Tenant-ID: $TENANT_ID" "$QUERY_URL/v1/traces/$BUFFERED_TRACE_ID")
echo "$RESULT" | jq .
```
*Pass Criteria:* The trace buffered during the outage is successfully retrieved from ClickHouse.

---

### Drill 3: PostgreSQL Database Disruption & Auth Failover

**Objective:** Verify that PostgreSQL disruptions fail closed on security boundaries (invalidating or refusing unknown tokens) and that connection pools recover automatically when the database returns.

#### Step 1: Simulate PostgreSQL Disruption
Suspend instances in the CloudNativePG cluster:
```bash
kubectl patch cluster postgres -n "$NAMESPACE" --type merge -p '{"spec":{"instances":0}}'
kubectl wait --for=delete pod -l cnpg.io/cluster=postgres -n "$NAMESPACE" --timeout=60s
```

#### Step 2: Verify Auth & Ingest Fail-Closed
```bash
AUTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 \
  -X POST "$INGEST_URL/v1/traces" \
  -H "Authorization: Bearer un-cached-fake-key" \
  -H "Content-Type: application/json" \
  -d '{"resourceSpans":[]}')

echo "Uncached Auth HTTP Status: $AUTH_STATUS"
```
*Pass Criteria:* HTTP 401 or 500/503 (requests cannot bypass authentication while the security directory is offline).

#### Step 3: Restore PostgreSQL Cluster
```bash
kubectl patch cluster postgres -n "$NAMESPACE" --type merge -p '{"spec":{"instances":1}}'
kubectl wait cluster/postgres --for=condition=Ready -n "$NAMESPACE" --timeout=180s
```

#### Step 4: Verify Pool Self-Healing
Wait 10 seconds for `sqlx` pools to reconnect, then execute an authenticated trace ingestion and query:
```bash
sleep 10
PG_TRACE_ID=$(openssl rand -hex 16)
curl -sf -X POST "$INGEST_URL/v1/traces" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"drill-pg-recovery\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$PG_TRACE_ID\",\"spanId\":\"7777777777777777\",\"name\":\"pg-recovered-span\",\"startTimeUnixNano\":\"$(date +%s)000000000\",\"endTimeUnixNano\":\"$(date +%s)000000001\",\"status\":{\"code\":1}}]}]}]}"

sleep 6
curl -sf -H "Authorization: Bearer $TOKEN" -H "X-Tenant-ID: $TENANT_ID" "$QUERY_URL/v1/traces/$PG_TRACE_ID" | jq .
```
*Pass Criteria:* Query returns HTTP 200 with the newly ingested span.

---

## 3. Drill Thresholds & Target Recovery SLAs

| Drill Category | Failure Type | Ingestion SLA | Query SLA | Max RTO | Max RPO |
|----------------|--------------|---------------|-----------|---------|---------|
| **Broker Interruption** | Redpanda pod restart | Fail-closed (HTTP 503) within 5s | Unaffected for historical data | < 2 min | 0 |
| **Storage Interruption** | ClickHouse pod restart | 100% accepted (buffered in Redpanda) | HTTP 503 during outage | < 5 min | 0 (up to retention window) |
| **Identity / Auth Interruption** | PostgreSQL restart | Fail-closed (HTTP 401/503) | Degraded (cached sessions only) | < 3 min | 0 |
| **Workload Rolling Restart** | `kubectl rollout restart deploy` | 0 dropped requests | 0 dropped requests | < 1 min | 0 |

---

## 4. Operational Sign-off Checklist

After completing failure drills, record the results in the cluster audit log:

- [ ] All simulated failure events generated expected alerts on the [Platform Health Dashboard](../platform-alerting.md).
- [ ] Ingest fail-closed assertions verified no unacknowledged data was accepted during Redpanda downtime.
- [ ] Telemetry buffered during ClickHouse downtime was completely drained and verified without duplicate corruption.
- [ ] Connection pools for all 7 microservices re-established database connections without requiring manual pod restarts.
- [ ] Pod resource utilization stayed within defined limits during catch-up spikes.
