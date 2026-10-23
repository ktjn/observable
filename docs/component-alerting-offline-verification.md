# Offline alerting image verification

This acceptance runs the actual `alert-evaluator` executable produced by the
[offline Rust OCI helper](../distribution/README.md#offline-rust-oci-image-builds), including its
background threshold worker and authenticated read APIs. It records a component runtime signal for
[ADR-035](../spec/adr/ADR-035-component-independence.md), without claiming full alerting acceptance.

## Build and identities

```bash
python scripts/build_component_rust_image.py alerting --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/alerting-image
```

The helper reconstructs committed workspace source with all 526 verified Cargo archives and locked
APT closures. Frozen release compilation uses a private Cargo home/target with RUN networking and
BuildKit cache reuse disabled. Inspected image digests, labels, platform and runtime configuration
must match the request. Base/frontend transport remains separate from RUN network isolation.

Native Docker Desktop acceptance on 2026-10-23 recorded:

| Evidence | Value |
| --- | --- |
| Source revision | `47d1baea1b0461f77a332ba454c22cbb46e456a0` |
| Platform | `linux/amd64` |
| Image digest/ID | `sha256:e0fa2b6957e47146eacb6bd6bfb78a8eded42c0c7aea488076f68e6954a1a0e3` |
| Context digest | `sha256:520201fddfe474420c7c13c42502cb3cde1855c3d2c4dfa44fa8fec1de0b6eed` |
| Recipe digest | `sha256:05cffba257c181f058ff3ae3ae0205242b0bf085cbb9473397eedf172b30f6ff` |
| Combined material digest | `sha256:8ef4d100e42d559f0c3b013817e02fb6a7e0cb24cffd095e1dd1a40254004f56` |
| Existing offline auth fixture image | `sha256:de8b304a88fdf3428969f1c34045d3c8336a7c285a18dba4a4bee95ffa930750` |
| Cached PostgreSQL fixture image | `sha256:06cad38a5d9f5d24b4d83d86def30795d5e4b757fedbf5281172b576dedcd941` |
| Cached ClickHouse fixture image | `sha256:42acb460c63b4e76b64bea7798b9e414178d59ef42eb92e86fe2abf7ceb6c51c` |

Dependency IDs were resolved locally from existing `postgres:18` and
`clickhouse/clickhouse-server:26.9` fixture tags; these are recorded test inputs rather than new
authenticated production pins. Full identity and publication flags remain false.

## Runtime and fixture

With networking disabled, the exact image passed UID/GID `65532:65532`,
`SERVICE=alert-evaluator`, original shell exec entrypoint and executable binary greater than 1 MiB.
Service/curl linkage, curl execution, nonempty CA bundle, all 18 locked runtime package versions
and exact committed proto bytes passed.

Use uniquely named containers and an internal Docker network, no published ports or persistent host
data, immutable dependency IDs with `--pull never`, and cleanup registration before container starts.
Use the previously documented disposable
[PostgreSQL/auth conventions](component-ingest-offline-verification.md#runtime-and-dependency-fixture)
and [ClickHouse tmpfs conventions](component-store-offline-verification.md#isolated-persistence-fixture).
Apply every committed PostgreSQL migration lexically with `psql -v ON_ERROR_STOP=1`; create the
ClickHouse `observable` database and apply all four committed ClickHouse SQL migrations.

Seed two unique tenants, public dummy member/viewer tokens and one threshold rule per tenant.
Qualify `control.tenants`, `auth.api_keys`, `alerting.alert_rules` and `alerting.alert_firings`
explicitly: migration 040 leaves public replay placeholders. Validate both tokens through the real
offline auth image's `/internal/validate`, asserting tenant, role and environment.

Both rules use the same unique metric name and condition `gt 0.05`, severity `warning`, zero
`for_duration_secs`, empty notification channels and `auto_trigger_incident=false`. Leave service
name null to avoid service-based suppression. This deliberately tests active/resolved behavior
without pending-duration, notification or incident side effects.

Insert current integer nanosecond metric points into ClickHouse `observable.metric_points` using
columns `tenant_id,metric_series_id,metric_name,service_name,time_unix_nano,value_double`. Tenant A
starts at `0.10`; tenant B at `0.02`, with distinct series UUIDs and the same metric name. Nullable
and histogram fields use schema defaults; a `metric_series` row is not needed for this worker.
Current timestamps avoid the table's 14-day TTL.

Start the exact alerting image through its original entrypoint with:

```text
OBSERVABLE_ENV=acceptance
DATABASE_URL=postgres://observable@OWNED_POSTGRES:5432/observable
AUTH_SERVICE_URL=http://OWNED_AUTH:4319
CLICKHOUSE_URL=http://OWNED_CLICKHOUSE:8123
CLICKHOUSE_USER=default
INTERNAL_SERVICE_TOKEN=public-dummy-internal-token
ALERT_EVAL_INTERVAL_SECONDS=1
```

Omit telemetry exporter endpoints. Require HTTP 200 from port 4322 `/health` and `/readyz`.
Readiness acquires a PostgreSQL connection and runs ClickHouse `SELECT 1`; it does not establish
schema validity, auth availability or worker success.

## Actual worker lifecycle and API checks

Poll independently queried PostgreSQL firing rows for at most 60 attempts with one-second waits:

1. Tenant A gets exactly one nonnil firing ID, `active`, value `0.10`, nonnull `firing_start` and
   null `resolved_at`. Tenant B has no firing despite sharing the metric name.
2. Missing-auth GET `/v1/alerts/rules` returns 401. Authenticated lists contain only each token's
   own rule: A is `active`/firing, B is `ok`/not firing, with the expected metric/operator/threshold.
   Fetching the other tenant's rule detail returns 404 in both directions.
3. Insert a strictly newer A sample `0.20`. Require the same single firing ID with updated value,
   increased `occurred_at` and unchanged `firing_start`. This proves another background evaluation
   and observed deduplication, rather than merely waiting. Tenant B still has no firing.
4. Insert a strictly newer A sample `0.01`. Require the same firing to become `resolved` with that
   value, nonnull `resolved_at` and unchanged onset. Actual detail HTTP 200 reports the expected
   condition/type and one matching resolved firing. List HTTP 200 reports `resolved`/not firing.
   Tenant B still has no firing.

All checks passed. The threshold worker selects the latest point by tenant and metric name; it
does not filter service/environment or impose a freshness cutoff. `occurred_at` is refreshed by
evaluation; `firing_start` is the stable onset field.

Finally stop only the owned ClickHouse container: readiness returned 503, liveness stayed 200 and
the process remained running. Acceptance completed with exit code 0. All four owned containers and
the internal network were removed; a separate Docker inventory confirmed no fixture leftovers.

## Coverage limits

This covers one zero-duration threshold lifecycle and the described API-key reads. It does not
establish pending-duration promotion, admin CRUD/session/OIDC authorization, notifications/incidents,
other rule types, suppression, external delivery, restart/retry durability, throughput, complete
tenant-security coverage, full cluster operation, other platforms or native arm64. Existing
Testcontainers/Compose regression gates remain separate signals. Complete material/toolchain
provenance, authenticated release history and publication races remain open;
`identity_verified` and `publication_authorized` stay false.

Related specification: [independent releases](../spec/12-deployment.md#1932-independent-component-releases).
