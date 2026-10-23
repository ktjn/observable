# Offline ingest image verification

This acceptance runs the actual `ingest-gateway` image from the
[offline Rust OCI helper](../distribution/README.md#offline-rust-oci-image-builds) against real
auth, PostgreSQL and Redpanda dependencies. It verifies the existing
[component boundary](../spec/adr/ADR-035-component-independence.md) and
[token environment binding](../spec/adr/ADR-028-ingestion-token-environment-binding.md), without
changing service code or granting release authority.

## Build and identities

```bash
python scripts/build_component_rust_image.py ingest --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/ingest-image
```

The helper reconstructs committed source with 526 verified Cargo archives and locked APT closures.
It compiles a frozen release with a private Cargo home/target, disables RUN networking and BuildKit
cache reuse, then binds inspected image configuration/digests/labels to the request. Image/frontend
transport remains separate from RUN network isolation.

Native Docker Desktop acceptance on 2026-10-23 recorded:

| Evidence | Value |
| --- | --- |
| Source revision | `80b989bfe64338836b447d275a7717e25b124924` |
| Platform | `linux/amd64` |
| Image digest/ID | `sha256:41a74885f32a29453f91019a7e5d89c508cb72dd6789e05161344d04c04839bf` |
| Context digest | `sha256:12052e1b3c4c03baf9f69b0404a9859bb37daa32ede95948baf6ee1318e00088` |
| Recipe digest | `sha256:653d86a04b64ec1e12a643b63c3b726cbae077a9c53a6e608249d7a848af32e5` |
| Combined material digest | `sha256:8ef4d100e42d559f0c3b013817e02fb6a7e0cb24cffd095e1dd1a40254004f56` |
| Existing offline auth fixture image | `sha256:de8b304a88fdf3428969f1c34045d3c8336a7c285a18dba4a4bee95ffa930750` |
| Cached PostgreSQL fixture image | `sha256:06cad38a5d9f5d24b4d83d86def30795d5e4b757fedbf5281172b576dedcd941` |
| Cached Redpanda fixture image | `sha256:c98c2f04a751e6646012cc701bac5183252cee44c88ba1dfa412e4c7124f2e89` |

PostgreSQL and Redpanda fixture IDs were resolved from the existing `postgres:18` and
`redpandadata/redpanda:v26.2.4` tags, not introduced as authenticated production pins. The auth
fixture is the previously recorded offline auth build. Both release flags remain false.

## Runtime and dependency fixture

In `--network none`, the exact ingest image passed UID/GID `65532:65532`, `SERVICE=ingest-gateway`,
the original shell exec entrypoint and executable binary greater than 1 MiB. Service/curl linkage,
curl execution, nonempty CA bundle, all 18 runtime package versions and exact committed proto bytes
also passed. No host binary or shared compiler cache supplied the service.

Create unique owned names on an internal Docker network with no published ports or persistent
host data. Register names before starting containers and remove them on failure as well as success.
Run the immutable dependency IDs with `--pull never`:

- PostgreSQL: tmpfs `/var/lib/postgresql`, user/database `observable`, and
  `POSTGRES_HOST_AUTH_METHOD=trust` for this disposable internal fixture only. This is not a
  deployment authentication recommendation.
- Redpanda: tmpfs data owned by `101:101`, one CPU, 512 MiB, overprovisioning and Kafka on port 9092
  advertised using the owned broker name. Create one-partition/one-replica `telemetry.raw.v1` and
  `deployment.markers.v1` topics after broker metadata readiness.

Poll PostgreSQL readiness, then run every committed `migrations/postgres/*.sql` in lexical order
using `psql -v ON_ERROR_STOP=1`. After migration 040, seed `control.tenants` and `auth.api_keys`
explicitly: unqualified SQL would address public replay placeholders instead of the real tables.
Use a unique tenant UUID and a deliberately public dummy token, storing its SHA-256 hash, role
`member` and environment `offline-acceptance`.

Run the real offline auth image with a password-free fixture `DATABASE_URL`,
`OBSERVABLE_ENV=acceptance`, a dummy `SESSION_SECRET`, explicit dummy `ZITADEL_CLIENT_ID`, and
`ZITADEL_ISSUER`, `ZITADEL_API_BASE`, `ZITADEL_REDIRECT_URI` pointing to unavailable loopback URLs.
No OIDC interaction is exercised. Omit telemetry exporter endpoints. Require readiness 200 and
call `/internal/validate` with the dummy token; assert the exact tenant, role and environment.
This verifies actual database-backed validation rather than substituting mock auth.

Run ingest through its original entrypoint with:

```text
OBSERVABLE_ENV=acceptance
REDPANDA_BROKERS=OWNED_BROKER:9092
INGEST_TOPIC=telemetry.raw.v1
DEPLOYMENT_MARKERS_TOPIC=deployment.markers.v1
AUTH_SERVICE_URL=http://OWNED_AUTH:4319
```

Ingest has no PostgreSQL connection. Require HTTP 200 from OTLP-port `4318/health` and platform-port
`4321/readyz`. Readiness fetches broker metadata with a two-second timeout; it does not probe auth,
topic existence, consumer assignment or successful publication.

## HTTP and raw-envelope checks

Send an OTLP JSON resource log containing current string-encoded nanosecond timestamps, INFO/9
severity, body `offline-acceptance-log`, service `offline-acceptance-service` and a unique
`acceptance.marker` attribute. Set resource `deployment.environment=untrusted-resource-env` so
token-bound stamping is tested against a conflicting client value.

- Describe the raw topic with `rpk topic describe telemetry.raw.v1 --format json`: the single
  partition's high watermark starts at 0.
- POST `/v1/logs` without Authorization and with an invalid dummy bearer token. Both return 401;
  the raw topic high watermark remains 0.
- POST the same JSON with the valid dummy bearer token. Require HTTP 200 and `{}`; watermark is 1.
- Consume the raw record from the start using `rpk topic consume telemetry.raw.v1 --num 1
  --offset start --format '%k\t%v\n'`, with a 60-second external timeout.
- Assert Kafka key and envelope tenant equal the authenticated tenant; envelope environment is
  `offline-acceptance`, envelope UUID is nonnil and receive time is at least the fixture timestamp.
- Assert payload type `logs`, one log, nonnil generated log UUID, and every other field: token-bound
  tenant/environment, exact timestamps/severity/body/service/attributes, empty host ID and null
  trace/span IDs and fingerprint. Resource attributes retain the conflicting client environment.

All checks passed for the recorded image. Finally stop only the owned broker: the same ingest
process remained running, platform readiness returned 503 and OTLP liveness remained 200. The
acceptance completed with exit code 0. All four owned containers and their internal network were
removed; a separate Docker inventory confirmed no fixture leftovers.

## Coverage limits

The observed offsets establish the two rejected requests did not append before the accepted
request. They do not establish exactly-once delivery or later offset durability. Broker restart,
producer recovery, auth outages/revocation/viewer roles, gRPC/protobuf/compression, traces/metrics,
deployment-marker consumption, rate/cardinality limits, downstream process/storage integration,
full cluster behavior, other components/platforms and native arm64 remain separate gates.
Complete material/toolchain provenance, authenticated history and publication races remain open.
`identity_verified` and `publication_authorized` stay false. Existing Testcontainers/Compose
regression signals are preserved; this acceptance specifically runs the built OCI executables.

Related specification: [independent component releases](../spec/12-deployment.md#1932-independent-component-releases).
