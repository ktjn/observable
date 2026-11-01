# Dependency Outage and Bounded Degradation Runbook

**Incident class:** Temporary outage of PostgreSQL, ClickHouse, or Redpanda

This runbook defines the bounded-degradation behavior of the Observable platform during dependency outages and provides step-by-step procedures for safe recovery without data loss.

## Failure Modes & Bounded Degradation Matrix

| Failed Dependency | Ingestion Impact | Query API Impact | Alerting Impact | Admin / Auth Impact |
| :--- | :--- | :--- | :--- | :--- |
| **ClickHouse** | Buffered in Redpanda (`telemetry.normalized.v1`); no data lost up to topic retention (24h default) | Returns 503 on trace/log/metric queries | Suspended evaluations; no false recoveries fired | Normal (runs on PostgreSQL) |
| **Redpanda** | Fails closed (503); clients/collectors buffer and retry | Traces/logs already in ClickHouse remain queryable | Normal (reads directly from ClickHouse) | Normal (runs on PostgreSQL) |
| **PostgreSQL** | Ingest continues if API keys are cached in `auth-service` | Core queries function (zero PG dependency); audit events buffer in memory | Suspends rule/SLO updates; evaluates active rules | Login and tenant CRUD return 503 |
| **OpenFGA** | No impact | Normal query operations succeed; admin checks fail closed | Internal rule evaluations continue | Mutating admin operations return 500/403 |
| **Zitadel** | Ingest via API keys unaffected | Active user sessions valid until expiration | No impact | New user logins and OIDC callbacks return 503 |

## Immediate Triage

1. **Run platform diagnostic doctor**:
   ```bash
   make doctor
   # Or in Kubernetes:
   kubectl exec -n observable -it deploy/admin-service -- /bin/sh -c "curl -fsS http://localhost:4324/health"
   ```

2. **Identify the unavailable dependency**:
   ```bash
   kubectl get pods -n observable -l app.kubernetes.io/part-of=observable-platform
   kubectl get pods -n observable -l app.kubernetes.io/part-of=observable-infra
   ```

## Recovery Procedures

### 1. ClickHouse Outage Recovery

**When ClickHouse recovers after an outage**:

1. Verify ClickHouse is accepting native and HTTP connections:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client --query "SELECT 1"
   ```
2. Verify table readiness:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --query "SELECT count() FROM observable.spans"
   ```
3. Restart `storage-writer` pods if connection backoff has reached maximum interval:
   ```bash
   kubectl rollout restart -n observable deploy/storage-writer
   ```
4. Verify `storage-writer` drains the backlog from `telemetry.normalized.v1`:
   ```bash
   kubectl logs -n observable deploy/storage-writer --tail=50
   kubectl exec -n observable -it deploy/redpanda -- rpk group describe storage-writer
   ```
5. Confirm query API resumes serving without container restart:
   ```bash
   curl -fsS http://localhost:8090/v1/traces?limit=1
   ```

### 2. Redpanda Outage Recovery

**When Redpanda recovers after an outage**:

1. Verify broker cluster health:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk cluster info
   ```
2. Verify all three telemetry topics exist and partitions are healthy:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk topic list
   ```
3. If topics were wiped during an unclean broker loss, recreate them immediately:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk topic create telemetry.raw.v1 -p 3
   kubectl exec -n observable -it deploy/redpanda -- rpk topic create telemetry.normalized.v1 -p 3
   kubectl exec -n observable -it deploy/redpanda -- rpk topic create deployment.markers.v1 -p 1
   ```
4. Restart ingest gateway and stream processor:
   ```bash
   kubectl rollout restart -n observable deploy/ingest-gateway deploy/stream-processor
   ```
5. Verify collectors resume delivering buffered batches with HTTP 200.

### 3. PostgreSQL Outage Recovery

**When PostgreSQL recovers after an outage**:

1. Check database connectivity:
   ```bash
   kubectl exec -n observable -it deploy/postgres -- pg_isready -U observable
   ```
2. Verify component schemas (`auth`, `control`, `alerting`):
   ```bash
   kubectl exec -n observable -it deploy/postgres -- psql -U observable -d observable \
     -c "SELECT table_schema, count(*) FROM information_schema.tables WHERE table_schema IN ('auth','control','alerting') GROUP BY table_schema;"
   ```
3. If migrations were interrupted, run the migration job:
   ```bash
   kubectl create job --from=cronjob/manual-migrate manual-migrate-$(date +%s) -n observable
   ```
4. Restart dependent control-plane services:
   ```bash
   kubectl rollout restart -n observable deploy/auth-service deploy/admin-service deploy/alert-evaluator
   ```

## Safe Startup / Restart Sequence

If the entire cluster was shut down or restarted, follow this strict dependency startup sequence:

```
Step 1: Storage Infrastructure
        PostgreSQL  -->  ClickHouse  -->  Redpanda

Step 2: Identity & Authorization
        Zitadel  -->  OpenFGA

Step 3: Migrations & Bootstrapping
        PostgreSQL Migrations  -->  ClickHouse Migrations  -->  Zitadel/OpenFGA Bootstrap

Step 4: Platform Data Plane
        auth-service  -->  storage-writer  -->  stream-processor  -->  ingest-gateway

Step 5: Control Plane & Experience
        admin-service  -->  alert-evaluator  -->  query-api  -->  frontend
```

## Verification

After completing dependency recovery, run the platform health suite:
```bash
# Verify all endpoints and health probes
make doctor

# Verify backup and restore consistency
bash scripts/backup-restore-drill.sh --with-clickhouse
```
