# Offline storage image verification

This acceptance exercises the actual `storage-writer` executable built by the
[offline Rust OCI helper](../distribution/README.md#offline-rust-oci-image-builds), including
normalized Kafka ingress and ClickHouse persistence. It uses the existing component design in
[ADR-035](../spec/adr/ADR-035-component-independence.md) and
[deployment specification](../spec/12-deployment.md#1932-independent-component-releases).

## Build and recorded identities

```bash
python scripts/build_component_rust_image.py store-clickhouse --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/store-image
```

The helper reconstructs committed workspace source, all 526 verified Cargo archives and the locked
builder/runtime APT closures. Its generated recipe compiles a frozen release with a private Cargo
home/target, disables RUN networking and BuildKit cache reuse, then inspects the image's digest,
labels, platform and runtime configuration. Base/frontend transport is separate from RUN isolation.

Native Docker Desktop acceptance on 2026-10-23 recorded:

| Evidence | Value |
| --- | --- |
| Source revision | `28c18ee30df4b8d09689535b8349ca47a57012c1` |
| Platform | `linux/amd64` |
| Image digest/ID | `sha256:03e5d9e00a448bfa33331a203f0e44315d4c3b0baaccc7683f5fa14d9d08afe4` |
| Context digest | `sha256:05846d8352f80d5467d1de9f473c8257041f0a2ebf8aff9a8c2779fc002e3a23` |
| Recipe digest | `sha256:fa36132c49d0171b9a27dfb67af323ce27f8b0300c86448086788273bf385945` |
| Combined material digest | `sha256:8ef4d100e42d559f0c3b013817e02fb6a7e0cb24cffd095e1dd1a40254004f56` |
| Cached Redpanda fixture image | `sha256:c98c2f04a751e6646012cc701bac5183252cee44c88ba1dfa412e4c7124f2e89` |
| Cached ClickHouse fixture image | `sha256:42acb460c63b4e76b64bea7798b9e414178d59ef42eb92e86fe2abf7ceb6c51c` |

Fixture IDs were resolved locally from the repository's `redpandadata/redpanda:v26.2.4` and
`clickhouse/clickhouse-server:26.9` tags. These are recorded test inputs, not newly authenticated
production pins. `identity_verified` and `publication_authorized` remain false.

## Runtime contract

Use the exact image ID in `result.json`. With `--network none`, check:

- UID/GID `65532:65532`, `SERVICE=storage-writer` and the original
  `/bin/sh -c 'exec /usr/local/bin/$SERVICE'` entrypoint.
- Executable service binary larger than 1 MiB; service and curl `ldd` output without missing libraries.
- Working curl, nonempty CA bundle, all 18 runtime package versions matching the approved APT lock,
  and every `/proto/otlp` file matching its committed source bytes.

All checks passed for the recorded image. No host binary or shared compiler cache supplied the
service executable.

## Isolated persistence fixture

Create a uniquely named internal Docker network and three uniquely named containers, with no host
ports or persistent host volumes. Use the immutable fixture IDs above, `--pull never`, and tmpfs for
Redpanda data and ClickHouse data/logs. Check the images' service UIDs/GIDs before assigning tmpfs
ownership; Redpanda used `101:101`. Register owned names for cleanup before starting containers so
failed starts cannot leave untracked resources.

For this disposable internal fixture only, start ClickHouse with `CLICKHOUSE_SKIP_USER_SETUP=1`.
The image entrypoint supports this setting; it preserves the default user with no password. This
is not a deployment authentication recommendation. Poll `clickhouse-client --user default --query
'SELECT 1'`, create the `observable` database and apply all four committed ClickHouse SQL migrations
in lexical order. This runs migrations explicitly as fixture setup; the storage image does not
run them automatically.

Start Redpanda with one CPU, 512 MiB, overprovisioning, Kafka on `0.0.0.0:9092` and its owned name as
the advertised broker. Poll `rpk --ignore-profile -X brokers=OWNED_BROKER:9092 cluster info`, then
create `telemetry.normalized.v1` with one partition and one replica.

Run the exact storage image through its original entrypoint with:

```text
CLICKHOUSE_URL=http://OWNED_CLICKHOUSE:8123
CLICKHOUSE_USER=default
REDPANDA_BROKERS=OWNED_BROKER:9092
NORMALIZED_TOPIC=telemetry.normalized.v1
```

Omit self-observability exporter endpoints to avoid requiring ingest. Require HTTP 200 from
`http://localhost:4320/health` and `/readyz`, using curl inside the storage container. `/health`
is unconditional; `/readyz` executes ClickHouse `SELECT 1`. Neither proves Kafka assignment,
schema readiness or persistence.

Publish one newline-delimited JSON
[NormalizedTelemetryBatch](../libs/domain/src/envelope.rs) using `rpk topic produce
telemetry.normalized.v1 --format '%v\n'`: empty `spans`, `series`, `points`, and one
[LogRecord](../libs/domain/src/log.rs). Use unique nonnil tenant/log UUIDs and current nanosecond
timestamps to avoid the log table's 60-day TTL. Set INFO severity, an object body, fixture/resource
attributes, explicit service/environment/host values, and null trace/span IDs and fingerprint.

Poll `observable.logs` by the unique tenant/log UUIDs for at most 60 attempts with one-second
intervals. Compare all 15 fields: decode body/attribute JSON strings and compare nanosecond values
as integers. The recorded image persisted the row with all fields preserved. One observed row is
not an exactly-once guarantee or an offset-commit assertion.

Finally stop only the owned ClickHouse container. The same storage executable remained running;
readiness returned HTTP 503 and liveness HTTP 200. Remove the owned storage/broker/ClickHouse
containers and network on success or failure. Recorded acceptance completed with exit code 0,
and a separate Docker inventory confirmed no owned containers or network remained.

## Coverage limits

This proves the recorded amd64 offline image can start, consume one normalized log and write it to
the migrated ClickHouse schema, and reports ClickHouse connectivity loss through readiness. It
does not prove Kafka outage recovery, ClickHouse restart/retry recovery, committed offsets across
restarts, exactly-once delivery, malformed-message handling, spans/metrics, throughput, retention,
full cluster operation or arm64. Existing Testcontainers and Compose regression gates remain
separate signals. Complete material/toolchain provenance, authenticated release history and
publication safety remain open; local image metadata grants no publication authority.
