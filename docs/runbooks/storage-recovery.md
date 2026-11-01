# Storage Writer and ClickHouse Recovery Runbook

**Incident class:** ClickHouse insert latency, storage-writer consumer lag, or disk saturation

This runbook guides operators through diagnosing and resolving telemetry storage failures, batch deduplication retries, and ClickHouse cluster degradations.

## When to use this runbook

Use this runbook when:
- `storage-writer` reports continuous write failures, retries, or ClickHouse timeout errors.
- Consumer lag on Redpanda topic `telemetry.normalized.v1` increases steadily.
- ClickHouse queries time out or fail with read-only/disk full errors.
- `observable-query` or `observable-alerting` report 500/503 querying `observable.spans` or `observable.metric_points`.

## Architecture Context

```
Redpanda: `telemetry.normalized.v1`
          |
          v (consumer group: storage-writer)
  [storage-writer]  (:4320)
          |
          v (ClickHouse Native/HTTP protocol)
    ClickHouse DB: `observable`
      - `observable.spans`
      - `observable.span_events`
      - `observable.logs`
      - `observable.metric_series`
      - `observable.metric_points`
```

1. `storage-writer` buffers batches from `telemetry.normalized.v1` by count and interval.
2. Writes are idempotent and deduplicated per batch token.
3. Redpanda consumer offsets are committed **only after** ClickHouse acknowledges the write.
4. If ClickHouse is down, `storage-writer` retries infinitely without dropping data, relying on Redpanda topic retention as the durable buffer.

## Immediate Triage

1. **Check storage-writer container logs and health**:
   ```bash
   kubectl get pods -n observable -l app.kubernetes.io/name=storage-writer
   kubectl logs -n observable deploy/storage-writer --tail=100
   ```

2. **Check consumer lag on `telemetry.normalized.v1`**:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk group describe storage-writer
   ```

3. **Check ClickHouse cluster liveness and disk space**:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --user default --password observable \
     --query "SELECT name, path, formatReadableSize(free_space), formatReadableSize(total_space) FROM system.disks"
   ```

4. **Check for long-running mutations or part merges**:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --user default --password observable \
     --query "SELECT query_id, elapsed, query FROM system.processes WHERE is_cancelled = 0"
   ```

## Mitigation & Recovery Paths

### Scenario A: ClickHouse Disk Exhaustion

**Symptom**: `storage-writer` logs `Code: 243. DB::Exception: Cannot write to file ... No space left on device`.

1. Verify disk capacity:
   ```bash
   kubectl get pvc -n observable -l app.kubernetes.io/name=clickhouse
   ```
2. Trigger immediate retention drops on old partitions (e.g. drop partitions older than 7 days):
   ```bash
   # Inspect existing partitions
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --query "SELECT table, partition, count() FROM system.parts WHERE active GROUP BY table, partition ORDER BY partition"

   # Drop oldest span partition if disk is critically full (>95%)
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --query "ALTER TABLE observable.spans DROP PARTITION '<oldest_partition>'"
   ```
3. Expand PVC in Kubernetes and update Helm `values.yaml` volume storage size.

### Scenario B: Consumer Lag Accumulation during ClickHouse Outage

**Symptom**: ClickHouse was offline or restarted; `telemetry.normalized.v1` accumulated millions of records.

1. Ensure ClickHouse is fully healthy:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client --query "SELECT 1"
   ```
2. Scale up `storage-writer` replicas to parallelize partition consumption:
   ```bash
   kubectl scale -n observable deploy/storage-writer --replicas=3
   ```
3. Monitor `storage_writer_batches_written_total` and watch consumer lag decrease:
   ```bash
   kubectl exec -n observable -it deploy/redpanda -- rpk group describe storage-writer
   ```
4. Once lag returns to near-zero, scale `storage-writer` back to standard replica count (2).

### Scenario C: Corrupted Part or Unfinished Migration

**Symptom**: `Code: 60. DB::Exception: Table observable.spans doesn't exist` or schema version mismatch.

1. Verify schema version in ClickHouse:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --query "SELECT max(version) FROM observable.schema_migrations"
   ```
2. Re-run migration job:
   ```bash
   kubectl get job -n observable -l app.kubernetes.io/name=migration-job
   kubectl create job --from=cronjob/manual-migrate manual-migrate-$(date +%s) -n observable
   ```

## Restore and Verify

1. Run an end-to-end trace query to verify read and write paths:
   ```bash
   kubectl port-forward -n observable service/query-api 8090:8090 &
   PF_PID=$!
   curl -fsS http://localhost:8090/health
   curl -fsS "http://localhost:8090/v1/traces?limit=1" \
     -H "Cookie: session=$ADMIN_SESSION"
   kill $PF_PID
   ```
2. Confirm row count increases on active ingestion:
   ```bash
   kubectl exec -n observable -it deploy/clickhouse -- clickhouse-client \
     --query "SELECT count() FROM observable.spans"
   ```

## Escalation Criteria

Escalate to data infrastructure team if:
- ClickHouse data directories suffer filesystem corruption requiring `clickhouse-repair`.
- Part merging enters permanent memory-limit failure (`Code: 241. Memory limit exceeded`).
- Offset commit fails repeatedly with non-recoverable Kafka consumer errors.
