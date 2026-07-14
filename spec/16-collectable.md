# Collectable — Edge Pipeline Tool

> **Status:** Moved to its own repository: [github.com/ktjn/collectable](https://github.com/ktjn/collectable).

Collectable used to live in this repository as an independent, standalone tool with no
runtime coupling to Observable — its only integration point was the OTLP ingest endpoint.
Because it is generic (useful with any OTLP-compatible backend, not just Observable) it has
been extracted to its own repository.

**Problem it solves:** Observable's ingest gateway accepts OTLP only (see
[ADR-001](adr/ADR-001-otel-external-contract.md)). Collectable bridges sources that cannot
emit OTLP natively — legacy applications, syslog, log4j2 appenders, MQTT brokers, managed
cloud services, and unstructured text files — via a guided web UI that compiles a small,
static Rust binary with the transport/parser/OTLP mapping baked in at compile time.

See the [Collectable repository](https://github.com/ktjn/collectable) for the full
specification, architecture, and usage docs. The decision rationale for building it as an
independent tool remains recorded in [ADR-022](adr/ADR-022-collectable-mediator.md).

## Relation to Other Specs and ADRs

| Document | Relation |
|---|---|
| [ADR-001](adr/ADR-001-otel-external-contract.md) | Collectable reinforces OTLP as the external contract; does not change it |
| [ADR-004](adr/ADR-004-rust-data-plane.md) | Collectable mediators are written in Rust, consistent with the data plane decision |
| [ADR-022](adr/ADR-022-collectable-mediator.md) | Decision rationale for building Collectable, and its extraction to its own repository |
| [spec/06-agents.md §10.1](06-agents.md) | Collectable listed as a pipeline component |
| [spec/00-market-analysis.md §4.1](00-market-analysis.md) | Log pipeline gap analysis referencing Collectable |
| [spec/01-overview.md](01-overview.md) | OTLP-only ingest policy references Collectable as edge transformation path |
