--------------------------- MODULE TelemetryDelivery ---------------------------
(***************************************************************************)
(* Finite model of the 0.2 telemetry.normalized.v1 process-to-storage        *)
(* durability boundary (spec/19-formal-verification.md, ROADMAP 0.2 formal   *)
(* verification gate). It verifies the durability, retry, duplicate, and     *)
(* backpressure assumptions the architecture actually makes -- not an        *)
(* exactly-once guarantee it does not provide.                               *)
(*                                                                           *)
(* Production correspondence:                                                *)
(*   Produce        -> observable-process publishes telemetry.normalized.v1   *)
(*   Consume        -> observable-store-clickhouse consumes the stream        *)
(*   Store          -> storage writes through its ClickHouse ownership bound. *)
(*   Commit         -> durable Redpanda consumer progress (offset ack)        *)
(*   WorkerCrash/Recover, Redeliver -> store process lifecycle / broker       *)
(*                     redelivery of uncommitted records                      *)
(*                                                                           *)
(* Duplicate policy (at-least-once, NOT exactly-once): `stored` counts the    *)
(* durable copies of each record, and Store increments that count. Redelivery *)
(* after a successful write but before commit therefore produces stored[r]>=2 *)
(* -- the actual guarantee, because the ClickHouse spans/logs/span_events/    *)
(* metric_points tables are plain MergeTree with no record-identity           *)
(* deduplication (only metric_series uses ReplacingMergeTree). The model does *)
(* not strengthen this to idempotent exactly-once.                            *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS N,               \* number of abstract normalized records (stream length)
          Capacity,        \* max uncommitted backlog admitted by the stream
          WorkerCapacity,  \* max records a single worker holds in flight
          MaxAttempts      \* bounded delivery attempts per record

Records == 1..N
Offsets == 0..N

VARIABLES produced, inFlight, stored, committed, storeAvailable, workerState, attempts

vars == <<produced, inFlight, stored, committed, storeAvailable, workerState, attempts>>

\* `stored[r]` is the number of durable copies of record `r` (0, 1, or more).
TypeOK ==
  /\ produced \in Offsets
  /\ inFlight \subseteq Records
  /\ stored \in [Records -> 0..MaxAttempts]
  /\ committed \in Offsets
  /\ committed <= produced
  /\ storeAvailable \in BOOLEAN
  /\ workerState \in {"running", "failed"}
  /\ attempts \in [Records -> 0..MaxAttempts]

\* Records that are produced, not yet acknowledged, and not held in flight.
Available == {r \in 1..produced : r > committed /\ r \notin inFlight}

Produce ==
  /\ produced < N
  /\ produced - committed < Capacity
  /\ produced' = produced + 1
  /\ UNCHANGED <<inFlight, stored, committed, storeAvailable, workerState, attempts>>

Consume(r) ==
  /\ workerState = "running"
  /\ r \in Available
  /\ Cardinality(inFlight) < WorkerCapacity
  /\ inFlight' = inFlight \cup {r}
  /\ UNCHANGED <<produced, stored, committed, storeAvailable, workerState, attempts>>

\* A store attempt either durably admits a copy of the record or fails without
\* acknowledging. Redelivery can call it again for the same record, so the
\* count may exceed 1 (at-least-once).
Store(r) ==
  /\ workerState = "running"
  /\ storeAvailable
  /\ r \in inFlight
  /\ attempts[r] < MaxAttempts
  /\ stored' = [stored EXCEPT ![r] = @ + 1]
  /\ attempts' = [attempts EXCEPT ![r] = attempts[r] + 1]
  /\ UNCHANGED <<produced, inFlight, committed, storeAvailable, workerState>>

\* Advance the durable offset only across a prefix that has at least one
\* durable copy of every record.
Commit(k) ==
  /\ workerState = "running"
  /\ k \in (committed + 1)..produced
  /\ \A r \in (committed + 1)..k : stored[r] >= 1
  /\ committed' = k
  /\ UNCHANGED <<produced, inFlight, stored, storeAvailable, workerState, attempts>>

StoreUnavailable ==
  /\ storeAvailable
  /\ storeAvailable' = FALSE
  /\ UNCHANGED <<produced, inFlight, stored, committed, workerState, attempts>>

StoreRecovered ==
  /\ ~storeAvailable
  /\ storeAvailable' = TRUE
  /\ UNCHANGED <<produced, inFlight, stored, committed, workerState, attempts>>

WorkerCrash ==
  /\ workerState = "running"
  /\ workerState' = "failed"
  /\ inFlight' = {}
  /\ UNCHANGED <<produced, stored, committed, storeAvailable, attempts>>

WorkerRecover ==
  /\ workerState = "failed"
  /\ workerState' = "running"
  /\ UNCHANGED <<produced, inFlight, stored, committed, storeAvailable, attempts>>

\* Broker redelivery of an uncommitted record without a worker crash.
Redeliver(r) ==
  /\ r \in inFlight
  /\ inFlight' = inFlight \ {r}
  /\ UNCHANGED <<produced, stored, committed, storeAvailable, workerState, attempts>>

Next ==
  \/ Produce
  \/ \E r \in Records : Consume(r)
  \/ \E r \in Records : Store(r)
  \/ \E k \in Offsets : Commit(k)
  \/ StoreUnavailable
  \/ StoreRecovered
  \/ WorkerCrash
  \/ WorkerRecover
  \/ \E r \in Records : Redeliver(r)

Init ==
  /\ produced = 0
  /\ inFlight = {}
  /\ stored = [r \in Records |-> 0]
  /\ committed = 0
  /\ storeAvailable = TRUE
  /\ workerState = "running"
  /\ attempts = [r \in Records |-> 0]

Spec == Init /\ [][Next]_vars

(***************************************************************************)
(* Safety properties.                                                        *)
(***************************************************************************)

\* Acknowledged progress never covers a record without a durable copy.
CommitImpliesDurability == \A r \in 1..committed : stored[r] >= 1

\* Committed offsets are a prefix of produced records (no skipped offset).
NoOffsetGapCommit == committed <= produced

\* Backpressure is explicit: the uncommitted backlog never exceeds Capacity.
BacklogBounded == produced - committed <= Capacity

\* A worker never holds more than WorkerCapacity records in flight.
InFlightBounded == Cardinality(inFlight) <= WorkerCapacity

\* Every committed record has a durable copy (no committed loss).
NoCommittedLoss == \A r \in 1..committed : stored[r] >= 1

(***************************************************************************)
(* Temporal properties.                                                      *)
(***************************************************************************)

\* No transition erases durable state: produced and committed never decrease
\* and stored counts never decrease. In particular a worker crash (which
\* clears only volatile in-flight state) cannot erase durable stream or
\* storage state.
DurableMonotonic ==
  [][ (\A r \in Records : stored[r] <= stored'[r]) /\ (committed <= committed')
      /\ (produced <= produced') ]_vars

\* While storage is unavailable, no write is admitted, so a failed storage
\* attempt cannot be acknowledged as complete.
OutageDoesNotStore ==
  [][ ~storeAvailable => stored' = stored ]_vars

=============================================================================
