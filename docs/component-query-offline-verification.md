# Offline query image verification

This acceptance runs the actual `query-api` executable from the
[offline Rust OCI helper](../distribution/README.md#offline-rust-oci-image-builds) against real
auth, PostgreSQL and ClickHouse. It verifies a log-read path and audit persistence under the existing
[component](../spec/adr/ADR-035-component-independence.md) and
[tenant isolation](../spec/adr/ADR-007-multi-tenant-isolation.md) decisions. No service or NLQ code changes.

## Build and identities

```bash
python scripts/build_component_rust_image.py query --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/query-image
```

The helper reconstructs committed workspace source with 526 verified Cargo archives and locked APT
closures. Frozen release compilation uses a private Cargo home/target with RUN networking and
BuildKit cache reuse disabled. Inspected image digests, labels, platform and runtime configuration
must match the request. Base/frontend transport is separate from RUN network isolation.

Native Docker Desktop acceptance on 2026-10-23 recorded:

| Evidence | Value |
| --- | --- |
| Source revision | `039b4468d4d8a9d7ecad494eb3ede416a95e48cd` |
| Platform | `linux/amd64` |
| Image digest/ID | `sha256:d2c66fe402e9361ec67fff3f12e0d538dadf0c5821374bdb2eaa1fd0e8d532ec` |
| Context digest | `sha256:e9920f6d0a802ff40dd8ff01275af6465fc13135157427c69c22462a8eff251f` |
| Recipe digest | `sha256:6fbe9829ab3256c38f181ab3d2b6e846f8613120af510f3cd6efd8d8ca540308` |
| Combined material digest | `sha256:8ef4d100e42d559f0c3b013817e02fb6a7e0cb24cffd095e1dd1a40254004f56` |
| Existing offline auth fixture image | `sha256:de8b304a88fdf3428969f1c34045d3c8336a7c285a18dba4a4bee95ffa930750` |
| Cached PostgreSQL fixture image | `sha256:06cad38a5d9f5d24b4d83d86def30795d5e4b757fedbf5281172b576dedcd941` |
| Cached ClickHouse fixture image | `sha256:42acb460c63b4e76b64bea7798b9e414178d59ef42eb92e86fe2abf7ceb6c51c` |

Fixture IDs resolve existing `postgres:18` and `clickhouse/clickhouse-server:26.9` tags locally;
they are recorded test inputs rather than new authenticated production pins. Complete release
identity and publication authorization remain false.

## Runtime and fixture

In `--network none`, the exact image passed UID/GID `65532:65532`, `SERVICE=query-api`, original
shell exec entrypoint and executable binary greater than 1 MiB. Service/curl linkage, working curl,
nonempty CA bundle, all 18 locked runtime package versions and exact committed proto bytes passed.

Use unique owned container/network names, an internal network, no host ports or persistent host
data, immutable fixture IDs with `--pull never`, and cleanup registration before starts. Follow the
existing [PostgreSQL/auth fixture conventions](component-ingest-offline-verification.md#runtime-and-dependency-fixture)
and [ClickHouse tmpfs conventions](component-store-offline-verification.md#isolated-persistence-fixture).
Apply every committed PostgreSQL SQL migration lexically with `psql -v ON_ERROR_STOP=1`; create
ClickHouse database `observable` and apply all four committed ClickHouse migrations.

Seed two tenants and public dummy member/viewer API tokens using schema-qualified `control.tenants`
and `auth.api_keys`; store SHA-256 token hashes. Validate both tokens through actual offline auth
`/internal/validate`, asserting their tenant, role and environment. No OIDC interaction is exercised.

Insert three logs per tenant into `observable.logs`. Both tenants deliberately share service
`offline-query-shared`, host, trace and span IDs, with interleaved current nanosecond timestamps.
Each has severity 9/13/17 and distinct log UUIDs, body/index attributes, observed timestamps and
non-null fingerprints; environments distinguish `tenant-a` and `tenant-b`. Store body/attributes/
resource attributes as JSON strings, including nested values. Current times avoid the 60-day TTL.
Assertions use Python integers, preserving timestamps above JavaScript's safe integer range.

Start the exact query image through its original entrypoint with:

```text
OBSERVABLE_ENV=acceptance
AUTH_SERVICE_URL=http://OWNED_AUTH:4319
CLICKHOUSE_URL=http://OWNED_CLICKHOUSE:8123
CLICKHOUSE_USER=default
ADMIN_SERVICE_URL=http://127.0.0.1:1
ALERT_EVALUATOR_URL=http://127.0.0.1:1
INTERNAL_SERVICE_TOKEN=public-dummy-internal-token
```

Leave LLM credentials and telemetry exporter endpoints absent. The tested log paths do not call
control or alerting, so unreachable URLs satisfy configuration without verifying those dependencies.
Require port 8090 `/health` and `/readyz` HTTP 200. Readiness executes ClickHouse `SELECT toInt64(1)`;
it does not depend on PostgreSQL and does not prove schema, auth availability or successful log reads.

## Log API and audit assertions

API-key requests use both bearer and matching `X-Tenant-ID`; this query middleware does not share
ingest/control's bearer-only API-key path. Verify:

- Missing credentials return 401; malformed tenant header returns 400; tenant A's key with B's
  header returns 403.
- Each tenant's `GET /v1/logs?service=offline-query-shared&facets=service_name,severity_number,environment,host_id`
  returns only its three logs, newest first. Every one of the 15 fields exactly matches inserted
  structured values; total and all four facet counts match that tenant only.
- A search with `limit=2` returns two rows but total/facet population remain three. Minimum severity
  13 returns WARN/ERROR; shared trace/span filters still return only A. RFC3339 nanosecond `from/to`
  bounds include the first two A records exactly, despite interleaved B timestamps. An absent
  service returns an empty result.
- `GET /v1/logs/tail` with `since_unix_nano` at A's first timestamp returns only its later two rows,
  oldest first. This cursor is strict `>`.
- `GET /v1/logs/{A-middle-ID}/context?window=1` returns A's three rows in chronological order;
  window 0 returns just the pivot. Cross-tenant pivot IDs return 404 in both directions. Interleaved
  timestamps and identical host/service prevent context isolation from passing by fixture accident.
- Independent PostgreSQL verification finds exactly 10 `public.query_audit_log` rows matching
  expected tenant/action/returned-row-count tuples for `log_search`, `log_live_tail` and
  `log_context`. Limited search audits count returned rows, not total matches. Rejected and
  foreign-pivot requests create no query-read audit.
- Independent `auth.credential_audit_log` verification finds exactly 15 API-key credential allows:
  12 for A, three for B, including the two direct token validations. Auth validates A's key before
  query rejects its mismatched B header, so that 403 still has an auth allow. Malformed-header 400
  and missing-credential 401 do not call auth in this sequence.

Both audit writers are best effort; HTTP 200 alone would not prove audit persistence. All assertions
above passed. Finally stop only the owned ClickHouse container: readiness returned 503, liveness
remained 200 and the process stayed running. Acceptance completed with exit code 0. All four owned
containers and their internal network were removed, confirmed by a separate Docker inventory.

## Coverage limits

Only the described API-key log paths are covered. Traces, metrics, discovery/reliability, control/
alerting enrichment, NLQ/IR/SQL quality, sessions/OIDC, complete tenant-security coverage, schema
upgrades, audit failure handling, restart/retry durability, throughput, full cluster behavior,
other platforms and native arm64 remain separate gates. Existing Testcontainers/Compose regression
signals remain unchanged. Complete material/toolchain provenance, authenticated release history
and publication races remain open. `identity_verified` and `publication_authorized` stay false.

Related specification: [independent releases](../spec/12-deployment.md#1932-independent-component-releases).
