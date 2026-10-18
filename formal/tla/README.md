# TelemetryDelivery.tla

Executable finite model of the 0.2 `telemetry.normalized.v1` process-to-storage durability boundary
(`spec/19-formal-verification.md`, ROADMAP 0.2 formal-verification gate). It checks the durability,
retry, duplicate, and backpressure assumptions the architecture actually makes — not an exactly-once
guarantee it does not provide. TLA+ is a development-time tool only; no Observable service depends on
it.

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

## Adding an invariant

1. Define the invariant as an operator over the state variables in `TelemetryDelivery.tla`.
2. Add its name under `INVARIANTS` in `TelemetryDelivery.cfg`.
3. Run TLC. A violation prints a counterexample trace; an invariant that holds leaves the state count
   unchanged.
4. If TLC finds a real architectural bug, add a focused regression or chaos scenario (see
   `spec/19-formal-verification.md`) rather than weakening the invariant.
