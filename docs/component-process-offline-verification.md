# Offline process image verification

This acceptance checks the [offline Rust image helper](../distribution/README.md#offline-rust-oci-image-builds)
with the native Kafka/CMake component, rather than inferring its behavior from the auth build.
The image build and runtime checks are separate evidence boundaries.

## Build inputs

Use committed source, all verified Cargo archives, and the matching committed builder/runtime APT
closures. The helper resolves one immutable Git revision and passes a canonical tar through Docker
stdin; all RUN steps disable networking and cache reuse.

```bash
python scripts/build_component_rust_image.py process --ref APPROVED_COMMIT \
  --cargo-bundle /fresh/cargo-bundle --builder-bundle /fresh/builder-apt-bundle \
  --runtime-bundle /fresh/runtime-apt-bundle --platform linux/amd64 --output /fresh/process-image
```

The service selection is `stream-processor`. Its committed rdkafka dependency enables `cmake-build`;
the locked builder closure supplies CMake. Cargo uses frozen resolution, verified directory sources
and a fresh private target/home. Neither a precompiled host binary nor a shared compiler cache is
used. Base/frontend transport is separate from RUN network isolation.

## Broker-loss defect and regression

The initial process image at source `6804b1be` built and delivered a normalized log, but stopping
its broker made the executable exit with status 1. Native diagnostics identified a nonfatal
`MessageConsumption(BrokerTransportFailure)` escaping the consumer loop; the same fault was
reproduced by a new Testcontainers test as `MessageConsumption(AllBrokersDown)`. That failed
acceptance is retained here rather than treating ordinary readiness/delivery as outage coverage.

Source fix `527e4368` retries only nonfatal broker transport, all-brokers-down and name-resolution
receive errors with a 500 ms delay. The same consumer, buffered envelopes and partition watermarks
are retained. Fatal, authentication/authorization, commit, subscription and unrelated errors still
propagate. The consumer now lives in the component library so the executable and integration test
exercise the same implementation.

The new stop/restart regression requires baseline delivery, the same task surviving the outage,
and delivery of a distinct envelope after restarting the same broker. A lost asynchronous offset
commit may legitimately replay the known baseline envelope; the test permits that at-least-once
behavior while still requiring the new envelope and exact tenant within its recovery deadline.
A JoinSet aborts test tasks on both normal cleanup and unwinding. Classifier unit tests preserve
fatal and unrelated error propagation.

All 24 active package tests passed, including both real-broker integration tests; the existing
ignored reachable-readiness test also passed when run explicitly. Clippy passed all targets with
warnings denied. The corrected immutable-image outage acceptance below also passed.

```bash
cargo test --locked --offline -p stream-processor
cargo test --locked --offline -p stream-processor --test http_readyz_integration -- --ignored
cargo clippy --locked --offline -p stream-processor --all-targets -- -D warnings
```

## Verified native result

Corrected source `527e4368338cc31b00d670edb7a4281304d35c9e` built process linux/amd64 with
network-disabled RUN steps and cache reuse disabled:

- Image: `sha256:6b2a02eb49556ced0e6648c466ae5095619e3cd868fb7fac05fb118811255c1f`.
- Context: `sha256:93ee620eded5dbfae25f0a3ffc0bd9968fe97ee1038bb76a0e5cf3f9555d89a7`.
- Existing cached Redpanda v26.2.4 test image:
  `sha256:c98c2f04a751e6646012cc701bac5183252cee44c88ba1dfa412e4c7124f2e89`.

The finished image passed its non-root user/entrypoint/SERVICE contract, real binary floor,
Git-locked runtime package versions, service/curl linkage, readable CA and exact proto-byte
checks. Broker-backed `/health` and `/readyz` both returned HTTP 200. The actual executable consumed
a raw log envelope, published one normalized log, stamped the envelope tenant, generated a nonnil
log UUID and preserved every other log field. After broker stop, the same process remained running,
readiness returned HTTP 503 and liveness remained HTTP 200. Owned containers and network were removed.

This records the image-level outage gate separately from the same-consumer restart/delivery
Testcontainers regression. Neither establishes complete pending-buffer or restart-offset durability.
Full release identity and publication authorization remain false.

## Runtime contract

Use the exact image ID in `result.json`, with no release tag or push. In a `--network none` container,
check UID/GID `65532:65532`, `SERVICE=stream-processor`, the original shell exec entrypoint and a real
executable service binary larger than 1 MiB (the existing component-image stub regression floor).
Run `ldd` on both the service binary and curl and reject any unresolved library. Check curl execution,
readable nonempty CA certificates, all 18 exact runtime package versions against the approved APT
lock, and every `/proto/otlp` file against its exact committed source bytes.

## Isolated broker and delivery

The existing Compose and stream-processor Testcontainers fixtures use
`redpandadata/redpanda:v26.2.4`. Resolve that cached tag to its local image ID before running it; this
records the test infrastructure without upgrading repository pins or asserting publisher provenance.
Create a unique internal Docker network, no published host ports, and a uniquely named broker with:

```text
redpanda start --smp=1 --memory=512M --overprovisioned
  --kafka-addr=0.0.0.0:9092 --advertise-kafka-addr=OWNED_BROKER_NAME:9092
```

Use a fresh tmpfs at `/var/lib/redpanda/data`, owned by the image's checked UID/GID `101:101`.
Poll `rpk --ignore-profile -X brokers=OWNED_BROKER_NAME:9092 cluster info`, with a bounded retry window.
Create `offline.raw.v1` and `offline.normalized.v1`, each with one partition and one replica. These
are isolated fixture topic names, not changes to the product's default telemetry topics.
Start the exact process image through its original entrypoint with:

```text
REDPANDA_BROKERS=OWNED_BROKER_NAME:9092
INGEST_TOPIC=offline.raw.v1
NORMALIZED_TOPIC=offline.normalized.v1
STORAGE_WRITER_URL=http://127.0.0.1:1
STREAM_PROCESSOR_BATCH_SIZE=1
```

Require HTTP 200 from `http://localhost:4323/health` and `/readyz` using curl inside the process
container. Readiness checks Kafka metadata connectivity; it does not prove consumer assignment,
topic existence or message delivery.

Produce one compact newline-delimited JSON [TelemetryEnvelope](../libs/domain/src/envelope.rs) to
the raw topic with `rpk topic produce offline.raw.v1 --format '%v\n'`. Use a single
[LogRecord](../libs/domain/src/log.rs): nil record tenant/log ID, a nonnil envelope tenant, fixed
nanosecond timestamps, INFO severity, body `offline-process-acceptance`, fixture attributes, and
explicit service/environment/host values. Optional trace/span IDs and fingerprint are null.
Consume one normalized record from the start of `offline.normalized.v1` using
`rpk topic consume offline.normalized.v1 --num 1 --offset start --format '%v\n'`, with an external
60-second timeout. Assert:

- Exactly the `spans`, `logs`, `series`, `points` arrays; one log and the other arrays empty.
- The log tenant equals the envelope tenant, replacing the input nil tenant.
- The generated log ID parses as a nonnil UUID.
- All other log fields, including timestamps, body, attributes and service/environment/host, are
  unchanged.

Finally stop only the owned broker, require process readiness HTTP 503, and require liveness to
remain HTTP 200. Remove only the owned process/broker containers and internal network, including
on failure. No host credentials, production data or persistent host volumes enter this acceptance.

## Coverage limits

This exercises the actual process executable's queue consumer, normalization and normalized Kafka
producer. The existing broker roundtrip test alone does not run those application paths. Log-only
input deliberately avoids the separate span-derived-metrics HTTP flush, so the unreachable fixture
writer is not evidence about storage behavior. Producer/handler outage recovery, durable offsets across process restarts, throughput, spans/metrics,
ingest/storage integration, full cluster operation, arm64 and native arm64 hardware remain separate
gates. Local image/result metadata grants neither complete release identity nor publication authority.

Related decisions: [ADR-035](../spec/adr/ADR-035-component-independence.md) and the
[deployment specification](../spec/12-deployment.md#1932-independent-component-releases).
