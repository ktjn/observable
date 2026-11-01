# Retention, Compaction & GDPR / Tenant Purge Workflows

This document specifies Observable's retention management, partition compaction cycles, and GDPR / multi-tenant hard-purge operational workflows across ClickHouse and PostgreSQL.

---

## 1. Multi-Tier Telemetry Retention in ClickHouse

Observable manages telemetry retention across two primary mechanisms in ClickHouse:

1. **Table-level TTLs with Automatic Partition Drops**
   - ClickHouse MergeTree tables use `TTL` clauses on event timestamp columns to discard expired data automatically during background merges.
   - Partitioning scheme:
     - `spans`: `PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(start_time_unix_nano)))`
     - `span_events`: `PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(timestamp_unix_nano)))`
     - `logs`: `PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(timestamp_unix_nano)))`
     - `metric_points`: `PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(time_unix_nano)))`
     - `metric_series`: `ReplacingMergeTree(created_at) ORDER BY (tenant_id, service_name, metric_name, attributes)`
   - Hot-tier default retention policies:
     - Spans & Span Events: 14 days
     - Logs: 60 days
     - Metric Points: 14 days

2. **Compaction and Background Merges**
   - ClickHouse executes background merges periodically based on storage engine settings.
   - Expired parts are compacted and dropped once all records in the part exceed the table's TTL threshold.
   - For immediate space reclamation across monthly partitions, operators can issue:
     ```sql
     OPTIMIZE TABLE observable.spans FINAL;
     OPTIMIZE TABLE observable.logs FINAL;
     OPTIMIZE TABLE observable.metric_points FINAL;
     ```

---

## 2. Tenant Retention & Ingest Quota Configuration

Tenant retention parameters are governed by the `tenant-config` model in `spec/system.yaml` and stored in PostgreSQL (`control.tenants`):

- `retention_days`: Desired retention window in days (default: `30`, configurable per tenant).
- `ingest_quota_per_sec`: Maximum rate of ingestion per second (default: `100` req/sec).

### API Endpoints (`admin-service`)

- **Get Tenant Config**:
  ```http
  GET /v1/tenants/:id/config
  Authorization: Bearer <tenant_admin_token>
  ```
  Returns `200 OK`:
  ```json
  {
    "tenant_id": "00000000-0000-0000-0000-000000000002",
    "retention_days": 30,
    "ingest_quota_per_sec": 100
  }
  ```

- **Update Tenant Config**:
  ```http
  PUT /v1/tenants/:id/config
  Authorization: Bearer <tenant_admin_token>
  Content-Type: application/json

  {
    "retention_days": 90,
    "ingest_quota_per_sec": 500
  }
  ```
  Returns `200 OK` with updated configuration.

---

## 3. GDPR & Tenant Hard-Purge Workflow

When a tenant offboards or exercises a GDPR "Right to be Forgotten" request, a hard purge removes all control plane records, metadata, access tokens, and telemetry across PostgreSQL and ClickHouse.

### Hard Purge Execution Path

```http
DELETE /v1/tenants/:id
Authorization: Bearer <tenant_admin_token>
```

#### 1. ClickHouse Telemetry Deletion
Mutations are dispatched asynchronously across all ClickHouse MergeTree tables:
```sql
ALTER TABLE observable.spans DELETE WHERE tenant_id = '{tenant_id}';
ALTER TABLE observable.span_events DELETE WHERE tenant_id = '{tenant_id}';
ALTER TABLE observable.logs DELETE WHERE tenant_id = '{tenant_id}';
ALTER TABLE observable.metric_points DELETE WHERE tenant_id = '{tenant_id}';
ALTER TABLE observable.metric_series DELETE WHERE tenant_id = '{tenant_id}';
```
ClickHouse lightweight delete mutations mark matching rows immediately and eliminate them during subsequent part merges.

#### 2. PostgreSQL Atomic Purge Transaction
Within a single transaction, the following relational artifacts are deleted in dependency order:
1. `notification_audit_log` & `notification_channels`
2. `incident_events` & `incidents`
3. `alert_firings`, `alert_rules`, and `slo_definitions`
4. `deployment_markers` and `change_events`
5. `semantic_annotations`
6. `saved_view_grants` and `saved_views`
7. `dashboard_grants`, `dashboard_panels`, and `dashboards`
8. `user_sessions`, `user_tenant_roles`, `api_keys`, and `projects`
9. `tenants` (the root tenant row)

#### 3. Verifiable Security Audit Log
Before committing the purge transaction, an immutable audit event is appended to `auth.credential_audit_log`:
```sql
INSERT INTO credential_audit_log
  (action, outcome, credential_hash, tenant_id, auth_method)
VALUES
  ('tenant_purge', 'allow', '<actor_user_id>', '<tenant_id>', 'tenant_admin');
```
This leaves an auditable cryptographic breadcrumb verifying who triggered the purge and when it completed, while all operational and customer telemetry is erased.

---

## 4. Verification and Testing

Integration tests in `services/admin-service/tests/postgres_tenant_purge_integration.rs` verify:
- Non-admin callers receive `403 Forbidden` when attempting to inspect or alter tenant retention or issue purges.
- Tenant config updates persist to PostgreSQL and roundtrip correctly.
- Calling `DELETE /v1/tenants/:id` deletes the tenant row, cascades cleanly across child tables, schedules ClickHouse deletions, and appends the `tenant_purge` audit record.
