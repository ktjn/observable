# Formal verification models

Finite TLA+ models for the 0.2 architecture (`spec/19-formal-verification.md`). TLA+ is a
development-time tool only: no Observable service depends on it and no Rust is generated from it.
This file documents the models in this directory; `TelemetryDelivery.tla` is covered first, then
`ComponentCompatibility.tla`.

| Model | Verifies |
|---|---|
| [`TelemetryDelivery.tla`](TelemetryDelivery.tla) | the `telemetry.normalized.v1` process-to-storage durability boundary (durability, retry, duplicate, backpressure) |
| [`ComponentCompatibility.tla`](ComponentCompatibility.tla) | the distribution's component-composition rules (manifest selection, storage-schema compatibility, upgrade/rollback) |

`TelemetryDelivery.tla`: executable finite model of the 0.2 `telemetry.normalized.v1`
process-to-storage durability boundary. It checks the durability, retry, duplicate, and backpressure
assumptions the architecture actually makes — not an exactly-once guarantee it does not provide.

## Tooling

| | |
|---|---|
| Tool | TLA+ tools (TLC) v1.7.4 |
| Artifact | `tla2tools.jar` |
| Download | <https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar> |
| sha256 | `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88` |
| sha1 | `bee4a54f3ee3d4afc347c3240ec2d9e93b075104` |
| Requires | a Java runtime (TLC 2.19, tested with Temurin/Microsoft JDK 21+) |

Pin the version and verify the digest before use; never fetch a mutable `latest` JAR.

## Running TLC locally

```bash
# from the repository root
bash scripts/run-formal-verification.sh
```

The wrapper downloads the pinned JAR into `.tla/` (gitignored) and verifies its sha256, runs every
`formal/tla/*.cfg` under a wall-clock budget (`TLC_TIMEOUT` seconds, default 300), and writes each
model's full output to `.tla/<model>.log`. On failure it prints the counterexample trace and the log
path, so the trace survives the run. It is the "Formal verification (TLC)" step of
`scripts/local-ci.sh`.

or directly:

```bash
java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC \
  -config TelemetryDelivery.cfg -metadir <tmp>/states TelemetryDelivery.tla
```

TLC exits non-zero on an invariant or liveness violation and prints the shortest counterexample.

## Configured bounds

`TelemetryDelivery.cfg` uses the smallest domains that still expose the interesting interleavings:

| Constant | Value | Meaning |
|---|---|---|
| `N` | 3 | abstract normalized records in the stream |
| `Capacity` | 2 | max uncommitted backlog admitted (backpressure) |
| `WorkerCapacity` | 1 | max records a worker holds in flight |
| `MaxAttempts` | 2 | bounded delivery attempts per record |

This explores the complete reachable state space: **510 distinct states** (1888 generated, depth 19)
on TLC 2.19. Increase `N`/`Capacity` only if a larger configuration finds materially different
interleavings.

## Invariants

| Invariant | Meaning |
|---|---|
| `TypeOK` | every variable stays in its finite domain |
| `CommitImpliesDurability` | acknowledged offsets never cover an unstored record |
| `NoOffsetGapCommit` | committed offsets are a prefix of produced records |
| `BacklogBounded` | the uncommitted backlog never exceeds `Capacity` |
| `InFlightBounded` | a worker never holds more than `WorkerCapacity` records |
| `NoCommittedLoss` | every committed record is durably stored |

## Temporal properties

| Property | Meaning |
|---|---|
| `DurableMonotonic` | no transition erases durable state — `produced`, `stored`, and `committed` only grow (this is `CrashDoesNotEraseDurableState`: a crash clears only volatile in-flight state) |
| `OutageDoesNotStore` | while storage is unavailable no write is admitted, so a failed storage attempt cannot be acknowledged (this is `StorageOutageDoesNotAcknowledgeLoss`) |

`RedeliveryPreservesIdentity` and `TenantIdentityPreserved` hold structurally: record identity is a
constant, so no transition can change it.

## Interpreting a counterexample

Break `Commit` by deleting its durability guard (`\A r \in (committed + 1)..k : stored[r] >= 1`),
then run TLC. It reports `CommitImpliesDurability` violated with a 3-state trace:

```text
State 1: <Initial predicate>            produced = 0, stored = <<0,0,0>>, committed = 0
State 2: <Produce>                      produced = 1, stored = <<0,0,0>>, committed = 0
State 3: <Commit>                       produced = 1, stored = <<0,0,0>>, committed = 1   <- committed with no durable copy
```

Read the trace top to bottom: each line names the transition and the resulting state. The violation
means the transition allowed acknowledgement of a record the storage boundary never admitted. Do not
commit the broken transition; the guard is the fix.

To confirm the model exposes at-least-once rather than exactly-once, add an invariant
`NoDuplicates == \A r \in Records : stored[r] <= 1` to the configuration: TLC violates it, because
`Store` can run twice for a redelivered record.

## Production correspondence

| TLA+ transition | Observable component / boundary |
|---|---|
| `Produce` | `observable-process` publishes `telemetry.normalized.v1` |
| `Consume` | `observable-store-clickhouse` consumes the normalized stream |
| `Store` | storage writes through its ClickHouse ownership boundary |
| `Commit` | durable Redpanda consumer progress (offset acknowledgement) |
| `StoreUnavailable` / `StoreRecovered` | ClickHouse availability during an outage |
| `WorkerCrash` / `WorkerRecover` | store process lifecycle |
| `Redeliver` | broker redelivery of an uncommitted record |

Modeled guarantees and assumptions:

- **At-least-once, not exactly-once.** `stored` counts durable copies and `Store` increments the
  count, so a record redelivered after a successful write but before commit ends up with
  `stored[r] >= 2`. This matches the implementation: the ClickHouse `spans`, `logs`, `span_events`,
  and `metric_points` tables are plain `MergeTree` with no record-identity deduplication (only
  `metric_series` uses `ReplacingMergeTree`), so duplicate delivery produces duplicate rows. The
  model does not strengthen this to idempotent exactly-once.
- **Backpressure is explicit.** `Produce` is disabled once the uncommitted backlog reaches
  `Capacity`, so no record silently disappears.
- **Retries are bounded.** `Store` is disabled once `attempts[r]` reaches `MaxAttempts`, so a
  sustained outage stalls `Commit` rather than acknowledging lost data.
- **Identity is stable.** Record identity is never reassigned, so redelivery and recovery preserve
  tenant/environment identity (the model has no transition that changes it).

## Implementation correspondence and known gaps

### Duplicate delivery is observable (spec vs implementation)

`spec/02-architecture.md` describes storage writers as performing "idempotent writes" and accepts
"at-least-once + idempotent write design". The ClickHouse writes for `spans`, `logs`, `span_events`,
and `metric_points` are plain `MergeTree()` inserts with no deduplication token or record-identity
key, so a redelivered record produces a **duplicate row**; only `metric_series` uses
`ReplacingMergeTree(created_at)`. The model therefore encodes at-least-once, not idempotent writes.

Reconciling the two — either add record-identity deduplication in the storage writer, or record the
duplicate-tolerant guarantee explicitly in the spec — is follow-up work, not done here.

## Regression boundaries

Each invariant constrains a concrete production boundary. This table maps them to the existing
regression signals, and records where a boundary has no direct test yet.

| Invariant / property | Production boundary | Concrete regression signal |
|---|---|---|
| `CommitImpliesDurability`, `NoCommittedLoss` | store acknowledges only after a durable write | `services/storage-writer/src/buffer.rs` `send_spans_durable_resolves_only_after_flush_succeeds`; `flush_retries_until_success_instead_of_dropping` |
| `OutageDoesNotStore` | ClickHouse outage during a write | `services/storage-writer/src/buffer.rs` `flush_retries_until_success_instead_of_dropping`; `services/storage-writer/tests/http_api_integration.rs` `storage_writer_readyz_returns_503_when_clickhouse_is_unavailable` |
| `BacklogBounded` | bounded buffer/backpressure before commit | `services/storage-writer/src/buffer.rs` `flush_on_count`, `flush_on_timeout` (bounded accumulation) |
| `NoOffsetGapCommit` | Redpanda consumer offset commit | queue boundary covered by `services/stream-processor/tests/redpanda_integration.rs`; no direct offset-gap test yet |
| `DurableMonotonic` | store process lifecycle (crash/recovery) | no direct crash test yet; `tests/e2e/chaos_smoke_unit.sh` is the closest chaos signal |
| `RedeliveryPreservesIdentity`, `TenantIdentityPreserved` | tenant/environment stamping across the queue | `services/stream-processor/tests/redpanda_integration.rs` `redpanda_container_preserves_tenant_id_and_payload_across_queue_boundary` |
| duplicate-at-least-once | `MergeTree` duplicate rows | no direct redelivery test yet; tracked in issue #768 |

Where a boundary has no direct test, the model is still the only automated check for that transition —
that is the point of the formal layer. Add a Testcontainers/integration/chaos regression when a
concrete gap matters (see `spec/19-formal-verification.md` Slice 3).

## ComponentCompatibility.tla

Executable finite model of the distribution's component-composition rules. It verifies the
compatibility rules that are executable today: the distribution manifest selects a component set, and
`observable-query` must never serve against a ClickHouse storage schema it does not support.

### Bounds

The constants are defined in the module (small finite sets):

| Constant | Value |
|---|---|
| `StoreVersions` | `{"s1", "s2", "s3"}` |
| `QueryVersions` | `{"q1", "q2"}` |
| `Schemas` | `1..3` |
| `StoreSchemaOf` | `s1→1, s2→2, s3→3` |
| `QuerySupports` | `q1→{1}, q2→{1,2}` |

This explores the complete reachable state space: **81 distinct states** (669 generated, depth 7) on
TLC 2.19.

### Invariants

| Invariant | Meaning |
|---|---|
| `TypeOK` | every variable stays in its finite domain |
| `InstalledManifestIsCompatible` | the declared distribution set is compatible (the store's schema is one the selected query supports) |
| `QueryOnlyRunsOnSupportedSchema` | query serves only while the applied schema is in its supported set — this also covers the rollback rule, since `SetQuery` refuses a version that cannot read the applied schema |

### Interpreting a counterexample

Three deliberately broken transitions are caught (revert them after inspecting):

- `Migrate` no longer stops query when the new schema is unsupported → `QueryOnlyRunsOnSupportedSchema`
  violated (query keeps serving on a schema it cannot read);
- `Install` drops its compatibility guard → `InstalledManifestIsCompatible` violated (the distribution
  admits an incompatible set);
- `SetQuery` drops its schema guard → `QueryOnlyRunsOnSupportedSchema` violated (a rollback lands on an
  unsupported schema).

### Production correspondence

| TLA+ | Observable |
|---|---|
| `manifest` | `distribution/manifest.yaml` (the pinned component set) |
| `running` | the component versions actually deployed |
| `schema` | `migrations/clickhouse/SCHEMA_VERSION` applied by the store |
| `queryRunning` | `observable-query` serving traffic |
| `StoreSchemaOf` | the schema a store version migrates to |
| `QuerySupports` | `services/query-api/clickhouse-schema-support.yaml` |

Per-component image publishing is not modeled (release publishing is still open), so version identity
is abstract. Upgrade/rollback are modeled as changing the running query version.

## Adding an invariant

1. Define the invariant as an operator over the state variables in `TelemetryDelivery.tla`.
2. Add its name under `INVARIANTS` in `TelemetryDelivery.cfg`.
3. Run TLC. A violation prints a counterexample trace; an invariant that holds leaves the state count
   unchanged.
4. If TLC finds a real architectural bug, add a focused regression or chaos scenario (see
   `spec/19-formal-verification.md`) rather than weakening the invariant.
