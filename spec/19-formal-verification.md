# TLA+ Formal Verification

> **Status:** Proposed implementation specification.
> **Scope:** Development-time verification of Observable's stateful distributed-system invariants.
> TLA+ is not a runtime dependency, an alternative implementation, or a replacement for integration,
> chaos, performance, or contract tests.

## Goal

Use TLA+ and TLC to verify distributed state transitions that are difficult to cover reliably with
example-based tests: durable telemetry handoff, duplicate delivery, storage outages, replay,
backpressure, acknowledgement, and component-version compatibility.

The formal models must verify architectural rules already owned by the specifications and ADRs.
They must not become a second implementation of Observable.

## Architectural fit

The 0.2 target establishes:

```text
ingest -> telemetry.raw.v1 -> process -> telemetry.normalized.v1 -> store -> ClickHouse
```

The first formal model verifies the process-to-storage durability boundary introduced by ADR-035
and ADR-009:

```text
specifications / ADR invariants
              |
              v
       formal/tla/*.tla
              |
              v
             TLC
              |
              +--> counterexample traces
              +--> CI gate
```

Rust remains the production implementation. Redpanda and ClickHouse remain the production
infrastructure.

### Non-goals

- No TLA+ execution in Observable services.
- No generation of Rust from TLA+.
- No model of Kafka/Redpanda internals.
- No model of ClickHouse internals.
- No attempt to prove network or disk hardware correctness.
- No duplicate implementation of OTLP parsing, normalization, enrichment, or storage schemas.
- No replacement for Testcontainers, Compose/kind E2E, or chaos tests.
- No claim of exactly-once delivery unless the architecture explicitly adopts it.

## First verification boundary: `TelemetryDelivery.tla`

Model a bounded set of normalized telemetry records moving from the durable
`telemetry.normalized.v1` stream into durable storage.

### Constants

Use small finite sets suitable for exhaustive exploration:

```text
Records
Partitions
StoreWorkers
```

Each record has an abstract stable identity. Payload content remains opaque.

### State

The first model should contain variables equivalent to:

```text
produced          records appended to the normalized stream
available         records available for consumption
inFlight          records currently handled by storage workers
stored            records durably admitted by the storage boundary
committed         consumer progress/offset acknowledgement
storeAvailable    whether the storage dependency accepts writes
workerState       running / failed / recovering
attempts          bounded delivery-attempt count for model inspection
```

If the implementation uses batched offsets, model ordering/commit position explicitly rather than
pretending acknowledgement is per-record.

## Transitions

Keep transitions independent so TLC explores crash and recovery interleavings.

### Produce

Append a normalized record to the durable stream.

Postconditions:

- the record becomes available for consumption;
- producer success means the durable stream has accepted it, not that ClickHouse has stored it.

### Consume

Move an available record or ordered batch into a worker's in-flight state.

Consumption alone must not advance durable acknowledgement past data that can still be lost.

### Store

When storage is available, write an in-flight record.

The model must support duplicate delivery. Storage behavior must therefore encode the actual
Observable guarantee: either idempotent record identity/deduplication or explicitly documented
duplicate-tolerant semantics.

Do not silently strengthen the architecture to exactly-once.

### Commit

Advance the durable consumer position only when every record covered by that commit satisfies the
required storage durability condition.

For partition-ordered streams, a commit must not skip an unresolved earlier offset.

### StoreUnavailable / StoreRecovered

Toggle storage availability independently of worker state. While unavailable, the durable stream
retains uncommitted normalized telemetry within its retention assumptions.

### WorkerCrash

Lose volatile in-flight worker state. Durable stream state and already durable storage remain.

A crash before commit must permit redelivery.

### WorkerRecover

Restore consumption from durable committed progress.

### Redeliver

Return an uncommitted record to an eligible consumable state. Redelivery may occur even when a
previous storage attempt succeeded but acknowledgement did not, which is the key duplicate-delivery
case.

## Safety properties

TLC must check at least:

- **TypeOK** — every variable remains inside its finite domain.
- **CommitImpliesDurability** — committed progress never covers a record that has not satisfied the
  storage durability condition.
- **NoCommittedLoss** — a record acknowledged as committed cannot disappear from both durable stream
  responsibility and durable storage.
- **NoOffsetGapCommit** — partition progress cannot advance past an unresolved earlier record.
- **CrashDoesNotEraseDurableState** — worker failure changes only volatile state.
- **StorageOutageDoesNotAcknowledgeLoss** — storage unavailability cannot cause failed writes to be
  acknowledged as complete.
- **RedeliveryPreservesIdentity** — retries represent the same logical record identity.
- **TenantIdentityPreserved** — the abstract tenant/environment identity attached before the
  normalized boundary cannot change during retry/recovery.
- **BoundedBackpressureIsExplicit** — when configured capacity is exhausted, the model follows an
  explicit reject/block/drop policy; no record silently disappears.
- **DuplicateSemanticsAreExplicit** — a crash between Store and Commit cannot create an undefined
  state. The model either proves idempotent outcome or exposes duplicate-at-least-once behavior.

## Liveness

Use fairness sparingly and only for guarantees Observable actually intends to make.

Initial property:

```text
record produced
/\ storage eventually remains available
/\ a worker eventually remains available
~>
record stored and committed
```

This property is conditional. It does not claim progress during an unbounded dependency outage or
after stream retention has been exceeded.

Model retention expiry only when Observable defines the exact loss/degradation contract for that
condition.

## Counterexample scenarios

The model must be capable of exposing at least:

1. consume -> commit -> storage write fails;
2. consume -> store -> worker crashes before commit -> redelivery;
3. consume batch -> later offset stored -> commit skips failed earlier offset;
4. storage unavailable -> worker retries -> accidental acknowledgement;
5. worker crash -> volatile in-flight state lost -> durable stream redelivers;
6. repeated retries -> duplicate storage without an explicit duplicate policy;
7. queue/backpressure limit reached -> silent record disappearance;
8. retry/recovery accidentally changes tenant/environment identity.

Architectural bugs found by TLC should become focused regression or chaos scenarios where practical.

## Repository layout

```text
formal/
  tla/
    TelemetryDelivery.tla
    TelemetryDelivery.cfg
    README.md
```

Keep formal models outside service source trees. They describe cross-component architecture.

The README must document:

- pinned TLA+ tool version and digest;
- local TLC command;
- configured state-space bounds;
- how to interpret a counterexample;
- how abstract transitions map to Observable components;
- how to add an invariant.

## Tooling

Pin the TLA+ distribution used by CI. Never retrieve a mutable `latest` JAR during validation.

Execution may be wrapped, but the underlying contract is equivalent to:

```bash
java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC \
  -config TelemetryDelivery.cfg TelemetryDelivery.tla
```

The wrapper must return non-zero on invariant/liveness failure, isolate TLC state from source
directories, use deterministic configuration, and enforce a CI timeout/state budget.

## CI integration

Add a dedicated `formal-verification` PR gate:

1. restore the pinned TLA+ tool;
2. run TLC against every committed configuration;
3. fail on invariant violation, unexpected deadlock, or liveness violation;
4. preserve TLC traces/logs as failure artifacts;
5. enforce a bounded runtime suitable for the PR fast path.

A larger state configuration may run nightly, but the small model remains required on PRs.

## Relationship to production tests

```text
TLA+                         Observable tests
--------------------------   -----------------------------------------
distributed state space      concrete Rust behavior
crash/interleaving search    Testcontainers/Compose/kind integration
architectural invariants     contract/schema tests
counterexample traces        chaos and regression scenarios
small exhaustive model       realistic Redpanda/ClickHouse behavior
```

For each formal transition, document its production correspondence:

- `Produce` -> process publishes `telemetry.normalized.v1`;
- `Consume` -> store consumes normalized telemetry;
- `Store` -> storage component writes through its ClickHouse ownership boundary;
- `Commit` -> durable Redpanda consumer progress;
- crash/recovery transitions -> store process lifecycle.

Do not create a Rust copy of the TLA+ state machine merely to claim conformance.

## Follow-up models

Add models only when they protect a concrete architectural boundary.

### `ComponentCompatibility.tla`

The 0.2 decomposition makes components independently versioned and the distribution manifest pins a
tested set. Model:

- independently released component versions;
- API/event contract compatibility ranges;
- ClickHouse storage-schema compatibility between store and query;
- distribution-manifest selection;
- upgrade and rollback transitions;
- rejection of unsupported combinations.

Safety properties should include: every installed distribution is a declared compatible component
set; query never starts against an unsupported storage schema; rollback never requires reading a
schema the rolled-back component cannot understand.

### `AlertDelivery.tla`

When the 0.6 reliability workflow is implemented, model alert evaluation, deduplication,
suppression, notification retry, acknowledgement, and incident creation. Avoid modeling provider
internals.

### `RetentionDeletion.tla`

When 0.5 retention/deletion semantics are concrete, model hot/cold retention transitions, deletion
requests, completion, retries, and audit state. Verify that a completed deletion cannot leave a
supported query path returning the deleted tenant data.

## Implementation slices

### Slice 1 — executable telemetry delivery model

- [ ] Add `formal/tla/TelemetryDelivery.tla`.
- [ ] Add bounded `TelemetryDelivery.cfg`.
- [ ] Encode the safety properties above.
- [ ] During development, deliberately break commit ordering/acknowledgement and verify TLC produces
      a useful counterexample; do not commit the broken transition.
- [ ] Document local execution and architecture mapping.

**Done when:** TLC exhaustively checks the configured finite state space and the model clearly states
the durability, retry, duplicate, and backpressure assumptions it verifies.

### Slice 2 — required CI gate

- [ ] Pin TLA+ tooling by version and digest.
- [ ] Add the `formal-verification` PR job.
- [ ] Preserve counterexample output on failure.
- [ ] Add a runtime/state budget.
- [ ] Add a larger nightly configuration only if it finds materially different interleavings.

**Done when:** a PR violating a telemetry-delivery invariant fails without starting the Observable
runtime stack.

### Slice 3 — implementation correspondence

- [ ] Map each formal transition to the relevant process/store/Redpanda implementation boundary.
- [ ] Map invariants to Testcontainers, integration, and chaos tests where practical.
- [ ] Add regression tests for bugs found by TLC.
- [ ] Document differences between the finite model and production Redpanda/ClickHouse behavior.

**Done when:** maintainers can identify which production behavior each formal invariant constrains.

### Slice 4 — component compatibility model

- [ ] Add `ComponentCompatibility.tla` as independent component releases and the distribution
      manifest become executable.
- [ ] Model supported version/schema ranges and upgrade/rollback transitions.
- [ ] Verify unsupported component combinations cannot be admitted by the distribution.

**Done when:** the distribution's component-composition rules have an executable finite model in
addition to contract and E2E tests.

## Acceptance criteria

Retain the formal-verification layer when:

- it models accepted Observable architecture rather than inventing new runtime semantics;
- TLC finds representative durability/order bugs when transitions are deliberately broken;
- production services have zero dependency on TLA+;
- CI is pinned, deterministic, bounded, and actionable;
- counterexamples translate into useful integration/chaos regressions;
- formal models stay materially smaller than the Rust/infrastructure implementation;
- ADRs and `spec/` remain authoritative for architecture.

If a model starts reproducing Redpanda, ClickHouse, or service implementation internals, reduce its
scope.
