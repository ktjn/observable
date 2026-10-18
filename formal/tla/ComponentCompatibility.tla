------------------------- MODULE ComponentCompatibility -------------------------
(***************************************************************************)
(* Finite model of the 0.2 distribution's component-composition rules         *)
(* (spec/19-formal-verification.md Slice 4, ROADMAP 0.2 formal item 4).       *)
(*                                                                           *)
(* It verifies the compatibility rules that are executable today: the         *)
(* distribution manifest selects a component set, and observable-query must   *)
(* never serve against a ClickHouse storage schema it does not support.       *)
(*                                                                           *)
(* Production correspondence:                                                *)
(*   manifest    -> distribution/manifest.yaml (the pinned component set)     *)
(*   running     -> the component versions actually deployed                 *)
(*   schema      -> migrations/clickhouse/SCHEMA_VERSION applied by the store *)
(*   queryRunning-> observable-query serving traffic                          *)
(*   StoreSchemaOf -> the schema a store version migrates to                  *)
(*   QuerySupports -> services/query-api/clickhouse-schema-support.yaml range *)
(*                                                                           *)
(* Upgrade/rollback are modeled as changing the running query version; the    *)
(* rule "never serve or roll back onto a schema the component cannot read" is *)
(* enforced. Per-component image publishing is not modeled (release          *)
(* publishing is still open), so version identity is abstract.               *)
(***************************************************************************)
EXTENDS Naturals

StoreVersions == {"s1", "s2", "s3"}
QueryVersions == {"q1", "q2"}
Schemas == 1..3

\* The schema a store version migrates the cluster to.
StoreSchemaOf == [v \in StoreVersions |-> IF v = "s1" THEN 1 ELSE IF v = "s2" THEN 2 ELSE 3]

\* The storage-schema versions a query version is tested against.
QuerySupports == [v \in QueryVersions |-> IF v = "q1" THEN {1} ELSE {1, 2}]

VARIABLES manifest, running, schema, queryRunning

vars == <<manifest, running, schema, queryRunning>>

\* A declared component set is compatible when the store's schema is one the
\* selected query version supports.
Compatible(m) == StoreSchemaOf[m.store] \in QuerySupports[m.query]

TypeOK ==
  /\ manifest \in [store: StoreVersions, query: QueryVersions]
  /\ running \in [store: StoreVersions, query: QueryVersions]
  /\ schema \in Schemas
  /\ queryRunning \in BOOLEAN

\* Install only a declared compatible component set, and start query against it.
Install(sv, qv) ==
  /\ StoreSchemaOf[sv] \in QuerySupports[qv]
  /\ manifest' = [store |-> sv, query |-> qv]
  /\ running' = [store |-> sv, query |-> qv]
  /\ schema' = StoreSchemaOf[sv]
  /\ queryRunning' = TRUE

\* Upgrade the store ahead of the manifest; the schema changes only on Migrate.
UpgradeStore(sv) ==
  /\ running' = [running EXCEPT !.store = sv]
  /\ UNCHANGED <<manifest, schema, queryRunning>>

\* Apply the store version's schema. Query keeps serving only if it supports the
\* resulting schema; otherwise it stops rather than serving wrong results.
Migrate ==
  /\ schema /= StoreSchemaOf[running.store]
  /\ schema' = StoreSchemaOf[running.store]
  /\ queryRunning' = (StoreSchemaOf[running.store] \in QuerySupports[running.query])
  /\ UNCHANGED <<manifest, running>>

\* Upgrade or roll back the query version. Refuse a version that cannot read the
\* current schema, so a rollback never lands on an unsupported schema.
SetQuery(qv) ==
  /\ schema \in QuerySupports[qv]
  /\ running' = [running EXCEPT !.query = qv]
  /\ queryRunning' = TRUE
  /\ UNCHANGED <<manifest, schema>>

\* Query stops (e.g. an operator draining it) without changing versions.
StopQuery ==
  /\ queryRunning
  /\ queryRunning' = FALSE
  /\ UNCHANGED <<manifest, running, schema>>

Next ==
  \/ \E sv \in StoreVersions, qv \in QueryVersions : Install(sv, qv)
  \/ \E sv \in StoreVersions : UpgradeStore(sv)
  \/ Migrate
  \/ \E qv \in QueryVersions : SetQuery(qv)
  \/ StopQuery

Init ==
  \E sv \in StoreVersions, qv \in QueryVersions :
    /\ StoreSchemaOf[sv] \in QuerySupports[qv]
    /\ manifest = [store |-> sv, query |-> qv]
    /\ running = [store |-> sv, query |-> qv]
    /\ schema = StoreSchemaOf[sv]
    /\ queryRunning = TRUE

Spec == Init /\ [][Next]_vars

(***************************************************************************)
(* Safety properties.                                                        *)
(***************************************************************************)

\* Every installed distribution is a declared compatible component set.
InstalledManifestIsCompatible == Compatible(manifest)

\* Query never serves against a storage schema it does not support. This covers
\* the rollback rule: a query version that cannot read the applied schema is
\* refused, so a rollback never lands on an unsupported schema.
QueryOnlyRunsOnSupportedSchema == queryRunning => schema \in QuerySupports[running.query]

=============================================================================
