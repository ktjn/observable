# Offline control image verification

This acceptance exercises the actual `admin-service` executable selected by the
[offline Rust image helper](../distribution/README.md#offline-rust-oci-image-builds) for component
`control`. It verifies a deployment-marker path through real auth, PostgreSQL and Redpanda.
It does not establish the complete target control boundary in
[ADR-035](../spec/adr/ADR-035-component-independence.md) or supersede the transitional
[admin extraction](../spec/adr/ADR-033-admin-service-extraction.md).

## Build and identities

```bash
python scripts/build_component_rust_image.py control --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/control-image
```

The helper consumes committed workspace source, all 526 verified Cargo archives and locked APT
closures. Frozen release compilation uses a private Cargo home/target with RUN networking and
BuildKit cache reuse disabled. Inspected image digests, labels, platform and runtime configuration
must match the request. Base/frontend transport is separate from RUN network isolation.

Native Docker Desktop acceptance on 2026-10-23 recorded:

| Evidence | Value |
| --- | --- |
| Source revision | `03fc54ae1434e1264e3fa41666396263b0e4c5da` |
| Platform | `linux/amd64` |
| Image digest/ID | `sha256:6d82e480f133ecec4841d68a2f5c27afeeeb861da09120faaadb5d799ad244d7` |
| Context digest | `sha256:d96a0168294b9dccca1a5641619d35967523c151c12c5fbc1e677ad43a03c895` |
| Recipe digest | `sha256:23e32b500c8a220e465df58540e2194d3206ba45f5bc7dd00b1247985f298481` |
| Combined material digest | `sha256:8ef4d100e42d559f0c3b013817e02fb6a7e0cb24cffd095e1dd1a40254004f56` |
| Existing offline auth fixture image | `sha256:de8b304a88fdf3428969f1c34045d3c8336a7c285a18dba4a4bee95ffa930750` |
| Cached PostgreSQL fixture image | `sha256:06cad38a5d9f5d24b4d83d86def30795d5e4b757fedbf5281172b576dedcd941` |
| Cached Redpanda fixture image | `sha256:c98c2f04a751e6646012cc701bac5183252cee44c88ba1dfa412e4c7124f2e89` |

PostgreSQL/Redpanda IDs resolve the repository's existing `postgres:18` and
`redpandadata/redpanda:v26.2.4` tags locally; they are recorded test inputs, not new authenticated
production pins. Full release identity and publication authorization remain false.

## Runtime and fixture

The exact image passed network-disabled runtime checks: UID/GID `65532:65532`,
`SERVICE=admin-service`, original shell exec entrypoint, executable binary greater than 1 MiB,
service/curl linkage without missing libraries, working curl, nonempty CA bundle, all 18 locked
runtime package versions and exact committed proto bytes.

Use the [isolated ingest fixture conventions](component-ingest-offline-verification.md#runtime-and-dependency-fixture):
unique owned internal network/container names, no host ports or persistent host data, tmpfs
PostgreSQL/Redpanda, immutable fixture IDs with `--pull never`, and real offline auth. Apply every
committed PostgreSQL SQL migration in lexical order using `psql -v ON_ERROR_STOP=1` before seeding.
Explicitly seed `control.tenants` and `auth.api_keys`, avoiding public replay placeholders. Use a
unique tenant and two deliberately public dummy tokens with SHA-256 hashes, roles `member` and
`viewer`, and environment `offline-acceptance`. Validate both through actual auth
`/internal/validate`, asserting tenant/role/environment. Omit telemetry exporter endpoints and
use explicit dummy OIDC/session configuration; no OIDC interaction is exercised.

Create `deployment.markers.v1` with one partition/replica. Start the control image through its
original entrypoint with:

```text
OBSERVABLE_ENV=acceptance
DATABASE_URL=postgres://observable@OWNED_POSTGRES:5432/observable
AUTH_SERVICE_URL=http://OWNED_AUTH:4319
REDPANDA_BROKERS=OWNED_BROKER:9092
DEPLOYMENT_MARKERS_TOPIC=deployment.markers.v1
INTERNAL_SERVICE_TOKEN=public-dummy-internal-token
CLICKHOUSE_URL=http://127.0.0.1:1
CLICKHOUSE_USER=default
```

The deployment handlers and readiness do not call ClickHouse. Its deliberately unreachable URL
satisfies startup configuration without implying ClickHouse acceptance. Require `/health` and
`/readyz` HTTP 200 on port 4324. Readiness acquires a PostgreSQL connection only; it does not
verify schema, auth or broker availability.

## Deployment path and failure checks

Use a deployment body with service `offline-control-service`, environment `offline-acceptance`,
version `1.2.3`, explicit deployed-by/commit values and a unique public metadata marker. Calls use
bearer-only authentication without `X-Tenant-ID`.

- Require zero fixture-tenant rows in `control.deployment_markers` and topic high watermark 0.
- POST `/v1/deployments` without auth returns 401; viewer bearer returns 403. Rows/offset remain 0.
- Member POST returns 201 and a nonnil deployment UUID. Independently query PostgreSQL: every
  submitted field, tenant/ID, `in_progress` status and null optional project/finish/rollback fields
  match. Topic watermark is 1.
- Viewer PATCH of that deployment to `success` returns 403; persisted status and watermark stay
  unchanged. Member PATCH with explicit `finished_at=2026-10-23T12:00:00Z` returns empty HTTP 204.
- Member GET filtered by service/environment returns 200 and the expected single deployment,
  tenant, `success` status and exact finish time. Independently query the final PostgreSQL row,
  including all submitted fields and finish time. Topic watermark is 2.
- Consume both records from the start with `rpk topic consume deployment.markers.v1 --num 2
  --offset start --format '%k\t%v\n'` and a 60-second external timeout. Each key equals the tenant;
  event deployment/tenant IDs, service/environment/version match. Statuses are `in_progress`, then
  `success`, with exactly the seven event fields and publication timestamps within the fixture
  interval. Event `started_at_unix_nano` is publication time, including for finish events; it is
  not a copy of the persisted deployment start time.

All checks passed. Stop the owned broker, then POST another deployment with version `1.2.4`.
The request still returned 201 and every submitted field persisted under a new deployment ID.
Use a 15-second request limit to cover the producer's existing queue/delivery waits. Readiness and
liveness remained 200, and the process stayed running. This verifies the existing best-effort
event contract: PostgreSQL is authoritative; a failed event publish does not undo its write.
It does not establish eventual delivery of that failed event.

Acceptance completed with exit code 0. Registered owned names were cleaned on success/failure;
all four containers and the internal network were removed, confirmed by a separate Docker inventory.

## Coverage limits

Only the described API-key deployment path is covered. Browser sessions/OIDC, tenant switching,
other control APIs, internal-token routes, ClickHouse usage, event replay/recovery/outbox durability,
concurrency, throughput, other platforms and native arm64 remain separate gates. The deployment
body's environment is client-selected; ingest token-bound environment stamping does not apply here.
Shared database credentials, cross-owner SQL/foreign keys and remaining target control separation
are not resolved by this image acceptance. Complete toolchain/material provenance, authenticated
release history and publication races remain open. `identity_verified` and `publication_authorized`
stay false. Existing Testcontainers and Compose regression signals remain unchanged.

Related specification: [independent releases](../spec/12-deployment.md#1932-independent-component-releases).
