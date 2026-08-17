# Investigation: Operational System Model

## Status

Investigation only. This document captures the problem, baseline requirements, scope boundaries, and open questions. It intentionally avoids choosing a syntax, serialization format, or implementation approach.

## Problem statement

Current architecture descriptions, observability standards, and SLO specifications each cover part of the problem:

- topology models describe what components and relationships exist;
- OpenTelemetry describes observed runtime telemetry;
- OpenSLO describes service-level objectives;
- deployment platforms describe how workloads are instantiated and scaled.

What is missing is a vendor-neutral, machine-readable model for describing what a large distributed system is expected to look like operationally and validating that expectation against runtime reality.

The model should describe expected system structure, relationships, runtime envelopes, resilience expectations, observability requirements, and SLO references without becoming an API schema, deployment manifest, dashboard format, or platform configuration language.

The long-term goal is continuous comparison between declared architecture and observed runtime behavior.

```text
Declared architecture
        |
        +-- structure
        +-- expectations
        +-- constraints
        |
        v
Observed runtime state
        |
        v
Conformance / drift / violations
```

## Core principles

### Relationships are first-class operational contracts

A relationship is more than an edge between two nodes. It may carry expectations about:

- interaction mode;
- protocol class;
- criticality;
- timeout and retry behavior;
- traffic and capacity envelope;
- observability requirements;
- SLO references;
- ownership;
- security/trust boundaries;
- allowed or forbidden behavior.

### Systems are compositional units

The model must scale to very large estates. A system can contain subsystems, which can themselves contain additional subsystems and runtime elements.

A subsystem must be independently describable and must be able to expose a controlled public surface without requiring callers to understand its internal topology.

### Declared topology must be comparable with observed topology

The model should support validation against OpenTelemetry-derived runtime graphs and other observed state.

Examples:

- undeclared dependency observed;
- expected dependency missing;
- retired dependency still receiving traffic;
- required telemetry missing;
- replica count outside expected range;
- synchronous dependency observed where only asynchronous communication is allowed;
- timeout or retry behavior outside the declared contract.

### Describe expected runtime state, not deployment implementation

The model may describe that a workload is expected to have 2-6 replicas. It should not describe how Kubernetes HPA, ECS, Nomad, VM autoscaling, or another platform achieves that state.

### Compose existing standards rather than replace them

OpenSLO is a strong candidate for defining service-level objectives. OpenTelemetry is the natural source for observed runtime identity and telemetry.

API, event, model, and schema contracts are explicitly outside the core scope. They belong to dedicated contract systems such as Modelable.

## Base requirements

### 1. Hierarchical systems and subsystems

The model must support arbitrary composition.

```text
Enterprise
  +-- Payments
  |    +-- Cards
  |    +-- Accounts
  |    +-- Settlement
  +-- Lending
       +-- Origination
       +-- Servicing
```

Requirements:

- systems can contain systems;
- internal topology can be hidden from parents and consumers;
- public surfaces can be exposed explicitly;
- consumers depend on published surfaces rather than internal implementation details;
- internal topology can evolve without forcing changes to higher-level models;
- multiple abstraction levels can be projected from the same underlying model.

### 2. Federation and independent ownership

A massive system cannot depend on one central file or repository.

Requirements:

- subsystems can be defined in separate repositories or packages;
- independently owned models can be composed;
- references between models are stable;
- subsystem public surfaces can be resolved without loading their full internals;
- versioning and compatibility between independently published models must be possible;
- tooling must support partial loading and local validation.

A hard scaling requirement is:

> Tooling must be able to work with a subsystem without materializing the complete global architecture graph.

### 3. Encapsulation and public surfaces

Subsystems need an explicit boundary between implementation and public architecture.

The conceptual model should support:

- internal elements;
- exported/public elements or capabilities;
- imported dependencies;
- relationships crossing the subsystem boundary.

A caller should depend on a published surface, not on arbitrary internal elements.

### 4. External systems

External systems must be first-class model elements.

An external system may be:

- a third-party service;
- a vendor platform;
- a government or industry service;
- another organizational domain for which only a published surface is known;
- an unmanaged legacy system.

External systems may be modeled with partial knowledge. Their internal implementation does not need to be represented.

The model should support known boundary characteristics such as:

- stable identity;
- owner/vendor;
- interaction direction;
- protocol class;
- SLA/SLO expectations;
- rate limits or quotas;
- authentication class;
- trust boundary;
- observability limitations.

A dependency should ideally target a published surface regardless of whether the implementation behind that surface is local, federated, or external.

### 5. Stable identity

Every modeled entity and relationship needs a stable identity independent of display name and repository location.

This is required for:

- renames;
- federation;
- history;
- runtime correlation;
- ownership changes;
- cross-repository references;
- lifecycle transitions.

Names are presentation. Identity is part of the contract.

### 6. First-class relationships

Relationships must support more than source and destination.

Potential relationship characteristics include:

- source and target;
- direction;
- interaction mode: synchronous, asynchronous, streaming, batch;
- protocol class;
- criticality;
- runtime vs startup dependency;
- ownership;
- observability expectations;
- resilience expectations;
- expected traffic envelope;
- SLO references;
- trust/security boundary metadata;
- lifecycle;
- constraints.

Relationships should have stable identities of their own.

### 7. Operational expectations

Systems, runtime elements, and relationships should be able to declare expected operating envelopes.

Potential dimensions:

- latency;
- throughput;
- error rate;
- concurrency;
- burst characteristics;
- capacity;
- message/request size class;
- dependency cardinality;
- steady-state and peak expectations.

OpenSLO should own formal service-level objective semantics where appropriate. The topology model should reference SLOs rather than duplicate them.

### 8. Scaling and capacity envelope

The model must describe expected scaling ranges independently of the deployment platform.

Examples of expected properties:

- minimum replicas;
- maximum replicas;
- expected steady-state replicas;
- minimum healthy replicas;
- scaling unit: instance, worker, partition, shard, or equivalent;
- whether horizontal scaling is supported;
- expected capacity per scaling unit;
- hard capacity limits.

Example runtime validation:

```text
declared replicas: 2..6
observed replicas: 1   -> violation
observed replicas: 4   -> expected
observed replicas: 12  -> drift or capacity violation
```

The model must not describe platform-specific autoscaling resources or algorithms.

### 9. Availability topology

Replica count alone does not express resilience.

The model should be able to describe expected failure-domain distribution such as:

- number of availability zones;
- number of regions;
- tolerance for instance failure;
- tolerance for zone failure;
- active/passive or similar high-level placement expectations.

This should remain platform-neutral.

### 10. Failure and resilience semantics

Relationships should describe architectural failure expectations without prescribing implementation details.

Potential concepts:

- hard dependency;
- soft dependency;
- optional dependency;
- startup dependency;
- runtime dependency;
- degrade-on-failure;
- fallback expected;
- timeout required;
- bounded retries;
- circuit-breaking expectation;
- backpressure expectation.

### 11. Observability expectations

The model should describe what must be observable.

Potential requirements:

- tracing required/optional/not applicable;
- metrics required;
- logs required;
- context propagation required;
- expected telemetry coverage across a relationship;
- required identifiers or dimensions;
- expected termination of tracing at external boundaries.

The model should align runtime identities with OpenTelemetry conventions where possible.

### 12. Runtime topology validation

A primary use case is comparing declared topology against runtime telemetry.

Tooling should be able to identify:

- undeclared dependencies;
- missing declared dependencies;
- unexpected synchronous or asynchronous interaction;
- unexpected external calls;
- missing telemetry;
- identity mismatches;
- scaling drift;
- topology drift;
- lifecycle violations;
- resilience configuration mismatches where observable.

The specification should define enough semantics for validation without prescribing one telemetry backend or one validation engine.

### 13. Structure, expectations, and constraints

The model should distinguish at least three kinds of assertions.

#### Structure

Facts about intended architecture.

Example:

```text
A depends on B
```

#### Expectations

Normal or desired operating envelopes.

Example:

```text
replicas: 2..6
normal latency: 20..100 ms
```

#### Constraints

Rules that define architectural violations.

Example:

```text
A must never depend on C
tracing is required across A -> B
```

This distinction allows tooling to report drift with the correct severity and semantics.

### 14. Environment and context overlays

The same architecture may have different operational expectations in different contexts.

Examples:

- development;
- test;
- staging;
- production;
- region;
- site;
- disaster-recovery mode.

The model should support context-specific overlays or equivalent composition without duplicating the complete topology.

### 15. Lifecycle

Systems, elements, public surfaces, and relationships should support lifecycle states.

Potential states:

- planned;
- experimental;
- active;
- deprecated;
- retired.

Lifecycle must influence validation. Traffic to a deprecated relationship may be a warning; traffic to a retired relationship may be a violation.

### 16. Ownership

Ownership should be representable on:

- systems;
- subsystems;
- runtime elements;
- public surfaces;
- relationships;
- external boundaries.

Relationship ownership is important because dependencies often span teams and responsibility for the edge is not necessarily identical to ownership of either endpoint.

### 17. Security and trust boundaries

The core model should contain enough information for architecture-level security validation without becoming a security policy language.

Potential concepts:

- trust zones;
- external boundaries;
- privileged relationships;
- allowed communication direction;
- data sensitivity crossing a boundary;
- authentication class.

The exact security model remains an open question.

### 18. Extensibility

The core specification should remain small and allow organization-specific extensions.

Requirements:

- unknown extensions do not prevent processing of core semantics;
- extensions are namespaced;
- core tooling can preserve extensions even if it does not understand them;
- extensions must not silently redefine core semantics.

Typical extension examples might include business criticality, cost center, security classification, or organization-specific policy metadata.

### 19. Version control and tooling characteristics

Whatever representation is chosen must be:

- machine-readable;
- Git-friendly;
- diffable;
- reviewable;
- deterministic enough for automated validation;
- composable;
- suitable for static CI validation;
- suitable for runtime validation;
- independent of diagramming tools;
- independent of observability vendor;
- independent of deployment platform.

## Primary use cases

The initial model should support these use cases cleanly:

1. architecture review before deployment;
2. CI validation of architectural rules;
3. comparison of declared topology with OpenTelemetry-derived topology;
4. architecture drift detection;
5. generation of architecture diagrams and dependency views;
6. dependency and SLO impact analysis;
7. blast-radius analysis;
8. detection of missing observability;
9. answering what a system depends on;
10. answering which systems depend on a given system;
11. determining whether production behaves according to declared architecture;
12. validating expected replica/scaling ranges;
13. local subsystem analysis without loading the entire enterprise graph;
14. viewing architecture at multiple abstraction levels.

## Explicit non-goals for the initial investigation

The following should stay outside the core scope unless later evidence shows they are required:

- API endpoint definitions;
- request/response schemas;
- event payload schemas;
- DTO/model definitions;
- schema evolution and compatibility;
- OpenAPI or AsyncAPI semantics;
- Kubernetes manifests;
- Terraform or infrastructure provisioning;
- autoscaler implementation details;
- dashboards;
- alerts;
- log query languages;
- trace query languages;
- application package dependency graphs;
- detailed business process modeling.

The core specification may provide opaque references to external contract systems, but should not understand or reproduce their semantics.

## Standards and concepts worth reusing

### OpenSLO

OpenSLO appears to be a strong fit for formal SLO definitions. The topology model should investigate how to reference OpenSLO resources at system and relationship boundaries without duplicating SLO semantics.

### OpenTelemetry

OpenTelemetry is the natural basis for observed runtime identity, traces, metrics, and logs. The investigation should determine which semantic conventions provide sufficiently stable identifiers for correlating observed runtime elements and edges with declared architecture.

### Existing architecture models

C4/Structurizr, Backstage catalog models, and similar systems should be studied for useful concepts, but the initial conclusion is that they are not sufficient as the core model because the primary requirement is operational architecture validation rather than diagram generation or service cataloging.

## Outstanding questions

### Conceptual model

1. What is the minimum set of first-class entity types?
2. Is `System` sufficient as a recursive compositional unit, or is a separate runtime-element/service concept required?
3. What exactly constitutes a public surface of a subsystem?
4. Are public surfaces entities, named capabilities, relationship endpoints, or projections?
5. Should relationships always be first-class resources or can simple relationships use shorthand that expands to a first-class model?
6. How should runtime instances relate to logical architecture elements?
7. How should dynamic targets such as shards, partitions, workers, or ephemeral instances be modeled?

### Identity and federation

8. What form should stable IDs take?
9. Who owns namespaces?
10. How are IDs moved across repositories without breaking references?
11. How should externally published subsystem models be versioned?
12. Is semantic versioning appropriate for architecture contracts?
13. What constitutes a breaking change in a topology model?
14. How are cyclic references across independently published models handled?
15. How are unresolved external references represented during partial evaluation?

### Composition and overlays

16. What are the exact inheritance rules between parent systems and subsystems?
17. Which policies should inherit automatically?
18. How are overrides represented and validated?
19. How do environment/region overlays compose?
20. How are conflicting overlays resolved deterministically?
21. Can one subsystem participate in multiple higher-level compositions without duplication?

### Operational expectations

22. Which performance characteristics belong in the topology model versus OpenSLO?
23. Should normal ranges and hard constraints be separate concepts?
24. How should throughput, concurrency, burst capacity, and latency be represented without becoming an SLO language?
25. How should capacity-per-replica or capacity-per-partition expectations be represented?
26. How should expected dependency cardinality be expressed?
27. Should resource expectations such as CPU or memory class exist at all, or are they deployment concerns?

### Scaling

28. Is replica count a generic cardinality concept rather than a dedicated scaling concept?
29. How should non-replica scaling units such as shards, partitions, workers, consumers, or nodes be represented?
30. Should the model describe expected steady state separately from allowed min/max?
31. How should temporary autoscaling beyond the normal range be treated?
32. How should runtime scaling information be collected in a platform-neutral way?

### Resilience and failure

33. What failure semantics are sufficiently architecture-level and platform-neutral?
34. Should timeout and retry values be core semantics?
35. Should circuit-breaker and bulkhead expectations be modeled as capabilities rather than implementation settings?
36. How should fallback and degradation behavior be represented?
37. Can resilience expectations be validated reliably from telemetry alone?

### Availability topology

38. What generic failure-domain model can represent zones, regions, sites, racks, or other domains without becoming cloud-specific?
39. Should placement be expressed as expected cardinality per failure domain?
40. How should active/passive and quorum-based systems fit the model?

### Observability

41. What is the minimum observability contract?
42. Should telemetry requirements be defined on systems, relationships, or both?
43. Which requirements can actually be validated from OpenTelemetry?
44. How should sampling affect conformance checks?
45. How should expected-but-unobserved low-traffic relationships be distinguished from missing relationships?
46. How should tracing boundaries for external systems be represented?
47. How should metrics-only or logs-only systems be represented?
48. What correlation mechanism should connect declared IDs to OpenTelemetry resource attributes?

### SLO integration

49. Can OpenSLO represent all SLO semantics required here without extension?
50. Should relationships reference SLOs directly?
51. Can a dependency edge have an SLO distinct from the target system's own SLO?
52. How should transitive SLO impact be calculated?
53. Should SLO dependency analysis be standardized or left entirely to tooling?

### Constraints and validation

54. What is the distinction between expectation, warning threshold, and hard architectural constraint?
55. How are forbidden relationships represented?
56. How should validation severity be modeled?
57. Should constraints be inherited?
58. Should constraints be able to target classes/tags of systems rather than explicit IDs?
59. How should exceptions and temporary waivers be represented?
60. Should waivers have mandatory expiry dates and ownership?

### External systems

61. How much information should be required for an external system?
62. How is partial or uncertain knowledge represented?
63. Should external systems have different validation semantics from owned systems?
64. How are vendor-provided SLA characteristics represented relative to internal expectations?
65. How should a federated internal system differ from a true third-party external system?

### Security

66. How much security metadata belongs in the core model?
67. Are trust boundaries and communication direction sufficient for v1?
68. Should data classification crossing a relationship be core or extension metadata?
69. Should authentication class be represented without modeling credentials or configuration?

### Lifecycle

70. What lifecycle vocabulary should be standardized?
71. How should planned architecture interact with runtime conformance validation?
72. How long may deprecated relationships remain observed before becoming violations?
73. Should lifecycle transitions themselves be versioned events?

### Representation and authoring

74. What is the canonical semantic model independent of syntax?
75. Should the canonical serialized representation be JSON or another format?
76. Is YAML acceptable only as interchange rather than primary authoring?
77. Does the graph-centric nature justify a dedicated DSL?
78. Would an existing language such as HCL provide enough structure without inventing a parser ecosystem?
79. Can multiple authoring syntaxes compile into one canonical model?
80. How should source locations survive compilation so validation errors map back to authored files?

### Scale and tooling

81. What scale should the model explicitly target: thousands, tens of thousands, or hundreds of thousands of elements?
82. What operations must remain local and bounded in cost?
83. What indexes are required for reverse-dependency and blast-radius queries?
84. How should global views be produced from federated models?
85. Should repositories publish compiled model artifacts to a registry?
86. Can a registry operate entirely from offline snapshots?
87. How should stale federated models be detected?
88. How should model provenance and source commit identity be preserved?

## Suggested next investigation steps

1. Define the conceptual entity model without choosing serialization.
2. Define precise semantics for composition, public surfaces, and relationships.
3. Define the distinction between structure, expectations, and constraints.
4. Map the proposed semantics against OpenSLO and OpenTelemetry to identify overlap and gaps.
5. Build several paper models of very different architectures:
   - small web application;
   - microservice estate;
   - event-driven platform;
   - sharded/partitioned system;
   - multi-region active/active system;
   - enterprise model with hundreds of independently owned subsystems;
   - system with significant third-party dependencies.
6. Test whether each example can be evaluated locally without resolving the complete global model.
7. Only after the semantic model survives these examples, evaluate JSON, YAML, HCL, and a dedicated graph DSL as authoring representations.

## Working hypothesis

The useful abstraction is not another architecture diagram format. It is a composable operational architecture contract that can be continuously compared with observed runtime state.

```text
Federated declared model
        +
OpenSLO objectives
        +
OpenTelemetry observations
        |
        v
Continuous architecture validation
```

The syntax should remain an implementation detail until the semantics and scaling requirements are proven.
