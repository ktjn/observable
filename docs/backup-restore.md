# Backup, Restore, and Upgrades

## Backup

Observable stores data in two databases. Both must be backed up together for a
consistent restore.

### PostgreSQL

PostgreSQL holds tenant configuration, API keys, dashboards, alert rules, SLOs,
user accounts, and schema annotations.

```bash
# Docker Compose
docker compose exec postgres pg_dump -U observable observable > backup-pg-$(date +%Y%m%d).sql

# Kubernetes
kubectl exec deploy/postgres -- pg_dump -U observable observable > backup-pg-$(date +%Y%m%d).sql
```

### ClickHouse

ClickHouse holds spans, logs, and metrics. Backup strategy depends on data
volume and retention requirements.

```bash
# Docker Compose — export individual tables
docker compose exec clickhouse clickhouse-client \
  --query "SELECT * FROM observable.spans FORMAT Native" > spans.native

# For large datasets, use ClickHouse's built-in backup
docker compose exec clickhouse clickhouse-client \
  --query "BACKUP DATABASE observable TO Disk('backups', 'observable-$(date +%Y%m%d)')"
```

For production ClickHouse deployments, refer to the
[ClickHouse backup documentation](https://clickhouse.com/docs/en/operations/backup)
for S3-backed and incremental backup options.

### What to back up

| Data | Location | Priority |
|------|----------|----------|
| Tenant config, API keys | PostgreSQL | Critical |
| Dashboards, alert rules | PostgreSQL | Critical |
| User accounts, roles | PostgreSQL | Critical |
| SLOs, saved views | PostgreSQL | Important |
| Spans, logs, metrics | ClickHouse | Depends on retention policy |
| Zitadel identity data | PostgreSQL (`zitadel` database) | Critical |
| Redpanda topic data | Redpanda | Not required (transient queue) |

## Restore

### PostgreSQL

```bash
# Docker Compose
docker compose exec -T postgres psql -U observable observable < backup-pg-20260715.sql

# Kubernetes
kubectl exec -i deploy/postgres -- psql -U observable observable < backup-pg-20260715.sql
```

### ClickHouse

```bash
# From Native format export
docker compose exec -T clickhouse clickhouse-client \
  --query "INSERT INTO observable.spans FORMAT Native" < spans.native

# From ClickHouse backup
docker compose exec clickhouse clickhouse-client \
  --query "RESTORE DATABASE observable FROM Disk('backups', 'observable-20260715')"
```

### Restore order

1. Stop all Observable services.
2. Restore PostgreSQL.
3. Restore ClickHouse.
4. Re-run migrations to ensure schema is current (migrations are idempotent).
5. Start services.

## Upgrades

### Migration idempotency

All PostgreSQL migrations use `CREATE TABLE IF NOT EXISTS`, `ALTER TABLE ... ADD
COLUMN IF NOT EXISTS`, and `ON CONFLICT DO NOTHING` patterns. All ClickHouse
migrations use `CREATE TABLE IF NOT EXISTS` and `ALTER TABLE ADD COLUMN IF NOT
EXISTS`.

This means:
- Migrations can be re-run safely without data loss.
- A restart during migration will not leave the database in a broken state —
  the next run re-applies from the beginning.
- There is no migration state table or version tracker; the migration runner
  simply applies all `.sql` files in order.

### Upgrade procedure

1. Back up PostgreSQL and ClickHouse.
2. Pull or deploy the new version's container images.
3. Run migrations:
   ```bash
   # Docker Compose
   make db-migrate
   
   # Kubernetes — migrations run as a Helm pre-install/pre-upgrade hook
   helm upgrade observable ./charts/observable -f values.production.yaml
   ```
4. Restart services (Docker Compose rebuilds; Helm rolling update).
5. Verify health endpoints respond on all services.

### Downgrade

Downgrading is **not supported** in Observable 0.1. Migrations are
forward-only and may add columns, tables, or constraints that older code does
not expect.

If a new version introduces a problem:
1. Restore from backup to the previous version's schema.
2. Deploy the previous version's container images.

Automated rollback tooling is planned for a future release.

## Failure behavior

| Scenario | Behavior |
|----------|----------|
| PostgreSQL unavailable | auth-service, admin-service, query-api, ingest-gateway, alert-evaluator fail health checks. Ingestion stops (API key validation fails). |
| ClickHouse unavailable | storage-writer fails health checks. Ingestion queues in Redpanda until ClickHouse recovers. Queries fail. |
| Redpanda unavailable | ingest-gateway accepts but cannot enqueue telemetry (rejects with 503). stream-processor stops processing. |
| Zitadel unavailable | Browser login fails. API-key ingestion continues working. |
| OpenFGA unavailable | Admin member operations fail. Other operations continue. |
| Single service crash | Other services continue. The crashed service's functionality is unavailable until restart. |
