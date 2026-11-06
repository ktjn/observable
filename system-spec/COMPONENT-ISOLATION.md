# Component Isolation and Programmable Capability Mocks

**Optional development methodology, with explicit obligations when adopted.** This guide extends the standalone-component workflow in [CONSTITUTION.md](CONSTITUTION.md). It does not prescribe a runtime architecture, language, network transport or test framework. Projects select enforcement and acceptance policies. The optional reference checker exercises some obligations but does not establish full isolation or behavioral equivalence. See [MODEL-SCOPE.md](MODEL-SCOPE.md).

## Goal

Every in-scope component must be independently buildable, startable/invocable and verifiable without running its real dependent components. Every in-scope component must also provide a **programmable mock of each capability it exposes**, so a consumer can develop and run without the real provider.

Both directions are essential:

- **Consumer isolation:** component A receives substitutes for every capability it requires. It has no need to run B, C or other real providers.
- **Provider substitution:** component A publishes a configurable mock for every capability it provides. Other components can use it without A's production implementation.

A mock is an implementation of a **contract**, not a copy of production code. The specification, requirements, risks and acceptance criteria remain authoritative; mocks and source code are replaceable artifacts. An isolation result is not an integration or production-readiness result.

## Definitions and boundaries

- **Standalone:** the component runs as an independently invocable library, executable, service, UI or other appropriate harness with local/in-memory substitutes for required capabilities. A standalone component need not be a separately deployable service.
- **Programmable mock:** a provider-owned, consumer-usable substitute that implements the same externally relevant interface and allows a test or caller to configure responses, state, delays, errors and interactions through a stable, documented control surface.
- **Fake:** a functional substitute (for example, an in-memory repository) that may implement realistic semantics without programmable expectations. A programmable mock may internally use a fake.
- **Contract suite:** tests/properties against the accepted capability contract, run against both the real provider adapter and its mock where applicable.
- **Composition/integration:** running components together against real providers; this is separate from isolation verification.

External/platform capabilities can use project-owned adapters or external simulators. The project must record any aspects of an external contract that cannot be simulated or verified. This workflow does not require modifying an externally owned implementation.

## Requirements when the methodology is adopted

| ID | Obligation | Acceptance criterion |
|---|---|---|
| ISO-01 | Independent execution | A documented command/harness can build and invoke the component without starting another real internal component; failures and unsupported environments are reported. |
| ISO-02 | Dependency substitution | Every declared required capability is injectable or otherwise replaceable at the consumer boundary; no hidden real-provider dependency is needed for standalone execution. |
| ISO-03 | Provider-owned programmable mock | Every provided capability has a versioned, consumable mock exposing the same contract without loading or starting the real provider. |
| ISO-04 | Programmable scenarios | Mock supports declared success, domain failures, transport/timeout failures where applicable, delays, and stateful or repeated interactions required by the contract. Configuration is deterministic and scoped to an isolated scenario. |
| ISO-05 | Contract fidelity | Shared contract verification runs against the real provider and mock. Any unsupported behavior, assumptions or differences are recorded; mock fidelity is not inferred from file existence. |
| ISO-06 | State/test isolation | Scenarios can reset state and observations; parallel executions cannot leak expectations, credentials or state across cases. |
| ISO-07 | No production/mock coupling | The consumer builds and runs with provider contracts and mock only; the mock must not rely on the real implementation's internals, persistent production services or secrets. |
| ISO-08 | Both modes verified | CI or the project's adopted gate executes isolated consumer tests and mock/provider conformance checks, while separate composition tests verify real integration. |
| ISO-09 | Version and change control | Contract changes trigger impact analysis, mock updates and reruns of consumer and provider verification before affected behavior is accepted. |

A UI component might run in a component preview, a library through a local harness, and a service through a local process. No Docker, HTTP endpoint or test framework is inherently required. Dependency substitution may use constructor injection, ports/adapters, plugin registration, configuration or another implementation-specific mechanism.

## Programmable mock contract

A published mock should make the following possible when relevant to its accepted capabilities:

1. **Configure** named scenarios or programmable behavior for each operation, without changing the consumer.
2. **Control** success/failure outcomes, ordering, delays, retry and timeout responses, invalid or edge-case values as permitted by the contract.
3. **Observe** calls and effects (arguments, events, ordering, call counts) without leaking implementation-private state.
4. **Reset** all scenario state, call history and configured responses reproducibly.
5. **Version** its contract identity and expose sufficient setup information for consumers to select the matching capability version.
6. **Fail predictably** when an interaction is unconfigured; do not silently return success or conceal unsupported contract behavior.

Mocks do not need to simulate implementation internals, internal databases or all deployment details. They must model externally meaningful capability semantics, especially invariants, consistency, idempotency and error behavior. Stateful or asynchronous contracts may require a controllable clock/scheduler or protocol model; list any unsupported semantics.

Mock configuration belongs to test/harness controls, not production-facing capability APIs unless the product explicitly requires it. Never expose test-control endpoints or bypasses in production.

## Example: checkout requires inventory

```text
checkout (real) ──requires reserve-inventory──> inventory (contract)
        │                                              │
        └────────── uses ─────────────────────── inventory mock
                                                       │
inventory (real) ───────────── same contract ──────────┘
```

The checkout harness runs without the inventory service. Its tests configure the published inventory mock for stock reserved, insufficient stock, timeout and duplicate retry. The inventory provider must itself run in isolation using substitutes for its own dependencies. The same capability contract tests exercise inventory-real and inventory-mock.

A possible language-neutral mock-control protocol:

```yaml
capability: reserve-inventory
contract_version: "1.0"
scenario:
  state: { available: 2 }
  responses:
    - on: reserve
      when: { quantity: 1 }
      result: { reservation_id: example-1 }
    - on: reserve
      when: { quantity: 3 }
      error: insufficient_inventory
  observations:
    record_calls: true
  reset_between_cases: true
```

This is an *illustrative scenario*, not an extension to the current machine-readable metamodel schema. The project chooses its mock API and scenario language.

## Lifecycle application

| Workflow | Isolation requirement |
|---|---|
| [GREENFIELD.md](GREENFIELD.md) | Specify provided capability mocks and consumer substitution alongside interface contracts; create mocks early enough for independent parallel component work. |
| [BROWNFIELD.md](BROWNFIELD.md) | Discover hidden dependencies, establish a component seam and characterize external behavior. Adopt standalone execution and provider mocks incrementally for each accepted scope; record exclusions and risks instead of pretending legacy components are already isolated. |
| [FEATURE.md](FEATURE.md) | Change the accepted contract first; update the provider mock, consumer scenarios and shared conformance tests before accepting implementation changes. |
| [REPLACEMENT.md](REPLACEMENT.md) | Replace a component without changing its consumers; the same published mock and contract suite remain available for parallel work, and the replacement must independently pass required verification. |

For AI agents, a standalone handoff contains the component's specification, required/provided contracts, mock controls, runnable harness, scenario tests, risk obligations and exact accepted versions. The agent must not require source or runtime access to other real internal components.

## Reference bindings and executable checks

The reference schema now accepts `Component.standalone_harness` (argv) and `Component.provided_mocks` (provided capability → adapter path, shared contract-suite argv, descriptive scenarios). Both are optional in ordinary validation. When a project adopts this methodology, execute:

```bash
system-spec examples/components/spec.yaml --check-isolation --root examples/components
```

The command **executes trusted repository code** and must only run in controlled CI/test environments. It sets `SYSTEM_SPEC_COMPONENT`, `SYSTEM_SPEC_MODE=standalone|real|mock`, `SYSTEM_SPEC_CAPABILITY` and `SYSTEM_SPEC_MOCK` for the selected run. The harness is run once per component. Each shared suite runs twice, once with `SYSTEM_SPEC_MODE=real` and once with `mock`, using the identical argv. A failing run, missing declared mock, missing dependency fake, invalid file path or timeout is reported. Commands use argv without shell expansion, run from the selected repository root and receive a minimal environment. This is **not a sandbox**: code can still reach network or filesystem resources granted to the runner. Use an isolated CI worker.

Sample bindings:

```yaml
components:
  - id: inventory
    version: "1.0"
    provides: [reserve-inventory]
    standalone_harness: [python, tests/isolation_runner.py]
    provided_mocks:
      reserve-inventory:
        path: tests/inventory/mocks/reserve_inventory.py
        contract_suite: [python, tests/isolation_runner.py]
        scenarios: [success, domain_failure, timeout, reset]
```

The checks establish execution of declared verification suites, **not** their adequacy or the absence of hidden real dependencies. `scenarios` is declared metadata only; suites must assert the actual scenario semantics and mock controls. Neither scenario completeness nor deterministic concurrency is automatically proved.

## Verification and limits in the current repository

- Existing `Component.fakes` (COMP-09) describes **consumer-owned** test doubles for required capabilities. `provided_mocks` separately describes provider-owned programmable substitutes.
- `COMP-10` requires a passing provider contract requirement when a capability is faked. The new opt-in checker runs the declared standalone command and shared contract suite in both modes, but it does not establish the suite's semantic sufficiency.
- `--check-artifacts` and `--check-drift` perform structural checks only. Neither establishes executable isolation, mock programmability, deterministic reset nor semantic parity.
- Do not repurpose `fakes` to mean provider-owned mocks. `provided_mocks` and `standalone_harness` are optional reference bindings; executable evidence still requires trusted CI and a project acceptance decision.

## Acceptance demonstrations

1. Start/invoke A with only its declared contracts and local dependency substitutes; prove that no real internal providers are started.
2. Start/invoke consumer B with A's published mock, without loading A's production implementation.
3. Configure success, expected domain failure and timeout/retry scenarios; inspect recorded calls and state.
4. Reset, repeat and parallelize scenarios; demonstrate deterministic isolation.
5. Run the same selected contract suite against A-real and A-mock, retaining evidence of differences and unverified cases.
6. Run separate integration/composition tests with real providers. These complement rather than replace standalone verification.

The desired invariant is **replaceability at both boundaries**: a component can be developed without its providers, and its consumers can be developed without it.
