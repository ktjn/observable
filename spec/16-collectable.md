# Collectable — Edge Pipeline Tool

> **Status:** Specced, not yet implemented.

---

## 1. Purpose and Scope

Collectable is an independent, standalone tool that lives in the Observable repository
but has no runtime coupling to Observable. Its sole integration point with Observable
is the OTLP ingest endpoint.

**Problem it solves:**
Observable's ingest gateway accepts OTLP only. This is an intentional offloading
strategy — parsing and transport complexity belong at the edge, not in the backend.
In practice, 80%+ of enterprise log volume originates from sources that cannot emit
OTLP natively: legacy applications, syslog, log4j2 appenders, MQTT brokers, managed
cloud services, and unstructured text files.

Existing tools (Fluent Bit, OTel Collector) can bridge this gap in theory but are
hard to configure correctly, especially the OTLP output mapping. Fluent Bit's OTLP
plugin requires manually deciding what becomes a resource attribute versus a log
attribute versus the body — there is no guidance and errors are silent. The OTel
Collector has a 50–150 MB footprint and its pipeline YAML is difficult to debug.

Collectable solves this with a **guided web UI** that walks the user through transport
selection, field extraction with live preview against sample data, and OTLP field
mapping. The result is a **compiled Rust binary** — not an interpreted config — with
the mapping baked in at compile time. The binary is small, static, and deployable
everywhere.

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────┐
│                  Collectable Builder                     │
│                                                         │
│  ┌──────────┐   ┌──────────────┐   ┌─────────────────┐ │
│  │ Parser   │   │ Build        │   │ Download        │ │
│  │ Dev UI   │──▶│ Service      │──▶│ Package         │ │
│  │ (React)  │   │ (Rust/cross) │   │                 │ │
│  └──────────┘   └──────────────┘   └─────────────────┘ │
│                        │                               │
│            Pipeline Definition (JSON)                  │
└────────────────────────────────────────────────────────┘
                          │
                 code generation
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│              Compiled Mediator Binary                    │
│                                                         │
│  Transport ──▶ Parser ──▶ OTLP Mapping ──▶ OTLP Emit   │
│                                                   │     │
└───────────────────────────────────────────────────┼─────┘
                                                    │
                                               OTLP/gRPC or HTTP
                                                    │
                                                    ▼
                                      Observable Ingest Gateway
                                       (or any OTLP backend)
```

**Key properties:**
- Transport and parser are **compiled in** — no runtime config interpretation.
- The pipeline definition is consumed at **build time**, not runtime.
- The mediator binary has **no knowledge of Observable** — it emits OTLP to
  whatever endpoint is configured.
- The builder tool runs via `docker compose up` with no external dependencies.

---

## 3. Pipeline Definition Schema

The pipeline definition is a versioned JSON (or YAML) document that fully describes
the mediator. It is the **source of truth** — stored in the user's version control,
not in Observable's control plane.

```json
{
  "version": "1",
  "name": "nginx-access-logs",
  "transport": {
    "type": "syslog_tcp",
    "port": 5140,
    "tls": false
  },
  "parser": {
    "type": "regex",
    "pattern": "(?P<client_ip>[\\d.]+) - - \\[(?P<timestamp>[^\\]]+)\\] \"(?P<method>\\w+) (?P<path>[^ ]+) HTTP/[\\d.]+\" (?P<status>\\d+) (?P<bytes>\\d+)"
  },
  "mapping": {
    "resource_attributes": {
      "service.name": { "literal": "nginx" },
      "host.name": { "env": "HOSTNAME" }
    },
    "log_attributes": {
      "http.method":      { "field": "method" },
      "http.target":      { "field": "path" },
      "http.status_code": { "field": "status", "type": "int" },
      "http.response_content_length": { "field": "bytes", "type": "int" },
      "net.peer.ip":      { "field": "client_ip" }
    },
    "body":          { "field": "$raw" },
    "severity_text": { "field": "status", "map": {
      "range_lt_400": "INFO",
      "range_lt_500": "WARN",
      "default":      "ERROR"
    }},
    "time_field":    { "field": "timestamp", "format": "02/Jan/2006:15:04:05 -0700" }
  },
  "output": {
    "endpoint": "${OTLP_ENDPOINT}",
    "protocol": "grpc",
    "headers": {
      "Authorization": "Bearer ${OTLP_TOKEN}"
    },
    "batch_size": 512,
    "flush_interval_ms": 1000
  }
}
```

Schema rules:
- `version` is required and must be `"1"` for v1 definitions.
- `transport.type` must be one of the supported transport identifiers (§4).
- `parser.type` must be one of the supported parser identifiers (§5).
- `mapping` fields support `literal`, `field`, `env`, and `map` value sources.
- `output.endpoint` and all string values support `${ENV_VAR}` interpolation,
  resolved at runtime in the compiled binary.

---

## 4. Transport Catalogue

| ID | Protocol | Crate | Notes |
|---|---|---|---|
| `syslog_tcp` | Syslog over TCP (RFC3164 / RFC5424) | `tokio` + `syslog-loose` | TLS optional |
| `syslog_udp` | Syslog over UDP | `tokio` + `syslog-loose` | |
| `http_webhook` | Generic HTTPS POST receiver | `axum` | Handles Firehose, Heroku log drains, Splunk HEC, generic webhooks |
| `mqtt` | MQTT topic subscriber | `rumqttc` | Pure Rust; musl-compatible; MQTT 3.1.1 and 5.0 |
| `kafka` | Kafka consumer group | `rdkafka` | Topic + consumer group config |
| `file_tail` | File tail (inotify / poll) | `tokio::fs` | Supports glob patterns, rotation detection |
| `stdin` | Standard input | `tokio::io` | Useful for pipe-based deployments |

**Transport selection policy:**
- `paho-mqtt` is excluded — it links to a C library and breaks static musl builds.
  Use `rumqttc` exclusively.
- The `http_webhook` transport handles multiple cloud log forwarding formats
  (AWS Firehose, Heroku log drain, Splunk HEC) via configurable payload extraction.

---

## 5. Parser Catalogue

| ID | Format | Crate | Notes |
|---|---|---|---|
| `json` | JSON objects (one per line or framed) | `serde_json` | Nested field access via dot notation |
| `grok` | Grok patterns (Elastic-compatible) | `grok-rs` | Supports built-in and user-defined patterns |
| `regex` | Named capture groups | `regex` | User-defined pattern; UI derives from sample |
| `key_value` | `key=value` pairs | `regex` | Configurable separator and delimiter |
| `multiline` | Multiline assembler | state machine | Start pattern triggers new record assembly |
| `log4j2_pattern` | Log4j2 PatternLayout string | derived regex | UI parses the pattern string, derives regex + field map |
| `log4j2_json` | Log4j2 JSONLayout output | `serde_json` | Maps `level`, `loggerName`, `message`, `instant`, `thread` to OTLP |
| `csv` | Delimiter-separated values | `csv` crate | Configurable delimiter; header row optional |
| `passthrough` | No parsing; raw string as body | — | Use when source already emits structured data |

**Parser/transport independence:**
Any transport can be combined with any parser. The pipeline definition selects one
of each; the build service generates a binary that composes them. There is no
hardcoded pairing.

---

## 6. OTLP Mapping Model

Every record produced by the parser is mapped to an OTel `LogRecord` before emission.
The mapping is defined in the `mapping` section of the pipeline definition.

### 6.1 Value Sources

| Source | Description | Example |
|---|---|---|
| `{ "field": "name" }` | Value of a parsed field | `{ "field": "status" }` |
| `{ "literal": "value" }` | Hardcoded string | `{ "literal": "nginx" }` |
| `{ "env": "VAR" }` | Environment variable at runtime | `{ "env": "HOSTNAME" }` |
| `{ "field": "name", "type": "int" }` | Parsed field coerced to integer | |
| `{ "field": "name", "map": {...} }` | Value mapped through a lookup table | Severity mapping |

### 6.2 Target Fields

| Target | OTel LogRecord field | Notes |
|---|---|---|
| `resource_attributes` | `Resource.attributes` | Applies to all records from this mediator |
| `log_attributes` | `LogRecord.attributes` | Per-record attributes |
| `body` | `LogRecord.body` | Use `{ "field": "$raw" }` for the full raw line |
| `severity_text` | `LogRecord.severity_text` | Maps to `severity_number` automatically |
| `severity_number` | `LogRecord.severity_number` | Direct integer assignment |
| `trace_id` | `LogRecord.trace_id` | 32-char hex string |
| `span_id` | `LogRecord.span_id` | 16-char hex string |
| `time_field` | `LogRecord.time_unix_nano` | Parses timestamp from a field |
| `drop` | — | Extracted field is discarded |

### 6.3 Severity Normalisation

When `severity_text` is set via a `map`, the compiler generates a match arm that
also sets `severity_number` according to the OTel severity number specification
(TRACE=1, DEBUG=5, INFO=9, WARN=13, ERROR=17, FATAL=21).

---

## 7. Builder Tool

### 7.1 Deployment

The builder tool is distributed as a Docker Compose application:

```bash
git clone https://github.com/ktjn/Observable
cd collectable
docker compose up
# Open http://localhost:8090
```

No other dependencies are required. The build service container includes the Rust
toolchain and `cross` for all supported targets.

### 7.2 UI Workflow

```
Step 1: Transport
  Select transport type → configure port / topic / file path / TLS → validate

Step 2: Sample Data
  Paste sample log lines (5–50 lines recommended)

Step 3: Parser
  Select parser type → define pattern / grok expression / field separator
  Live preview: each sample line is parsed and fields are displayed

Step 4: OTLP Mapping
  For each extracted field: assign to resource_attributes / log_attributes /
  body / severity / trace context / drop
  Live preview: resulting OTel LogRecord JSON displayed per sample line

Step 5: Output
  OTLP endpoint URL, protocol (gRPC / HTTP), auth header, batch size

Step 6: Download
  Select target ABI from dropdown
  Click "Build" → build service compiles and packages
  Download ZIP containing all artifacts
```

### 7.3 Build Service

The build service is a Rust HTTP service that:
1. Receives a pipeline definition (JSON) via POST `/build`
2. Validates the definition against the schema
3. Generates a Rust source package from templates
4. Invokes `cross build --release --target <abi>` in an isolated workspace
5. Assembles the download package (see §8)
6. Returns the package as a ZIP download

Compilation is isolated per request. Build artifacts are not persisted after download.

### 7.4 Supported Compilation Targets

| Target triple | Environment |
|---|---|
| `x86_64-unknown-linux-musl` | Linux x86-64, static binary (Alpine, containers, bare metal) |
| `aarch64-unknown-linux-musl` | Linux ARM64, static (Graviton, Apple Silicon containers) |
| `x86_64-unknown-linux-gnu` | Linux x86-64, glibc |
| `aarch64-unknown-linux-gnu` | Linux ARM64, glibc |
| `x86_64-pc-windows-gnu` | Windows x86-64 |
| `x86_64-apple-darwin` | macOS Intel |
| `aarch64-apple-darwin` | macOS Apple Silicon |

---

## 8. Download Package

The build service produces a ZIP archive with the following structure:

```
<name>-<version>-<target>/
  <name>                          # Compiled binary (or .exe on Windows)
  src/
    Cargo.toml                    # Pinned dependency versions; self-contained
    src/
      main.rs                     # Generated source — readable, auditable
  deploy/
    <name>.service                # systemd unit file, pre-configured
    init.d/<name>                 # SysV init script, pre-configured
    Dockerfile                    # FROM scratch (musl) or FROM alpine, COPY binary
    docker-compose.yml            # Drop-in snippet for existing Compose files
  pipeline.json                   # The pipeline definition used to generate this package
  README.txt                      # How to compile from source, deploy, configure
```

The generated source must produce an **identical binary** when built with
`cargo build --release --target <target>` using the pinned `Cargo.toml`.
No hidden build flags, no patches, no private registries.

---

## 9. Compiled Mediator Runtime Behaviour

### 9.1 Configuration

Config is read from two sources, merged in this order (later wins):

1. Config file: `collectable.toml` (or path set via `--config`)
2. Environment variables: `COLLECTABLE_<SECTION>_<KEY>` (e.g., `COLLECTABLE_OUTPUT_ENDPOINT`)

All `${ENV_VAR}` references in the pipeline definition are resolved from the
process environment at startup. Missing required variables cause an immediate
startup failure with a clear error message.

### 9.2 Logging

- Logs are written to **stdout** by default (compatible with Docker, systemd journal).
- Log level is set via `COLLECTABLE_LOG_LEVEL` (default: `info`).
- Log format is JSON by default for machine readability; `COLLECTABLE_LOG_FORMAT=text`
  enables human-readable output for development.

### 9.3 Signal Handling

| Signal | Behaviour |
|---|---|
| `SIGTERM` | Drain in-flight records, flush OTLP batch, exit 0 |
| `SIGINT` | Same as SIGTERM |
| `SIGHUP` | Reserved for future config reload |

Shutdown timeout is configurable (`COLLECTABLE_SHUTDOWN_TIMEOUT_SECS`, default 10).
Records received after the shutdown signal is received are dropped with a warning.

### 9.4 PID File

PID file writing is disabled by default. Enable with `--pid-file /var/run/collectable.pid`.
Required for SysV init script compatibility.

### 9.5 Health and Metrics

A minimal HTTP health endpoint is exposed at `GET /health` (port configurable,
default 9090). Returns `200 OK` when the mediator is running and the OTLP connection
is healthy.

Internal counters (records received, records exported, parse errors, export errors)
are exposed as basic prometheus-format metrics at `GET /metrics` on the same port.

---

## 10. Repository Layout

Collectable lives at the top level of the Observable repository as an independent
project. It has its own Rust workspace and is **not** a member of the Observable
Cargo workspace.

```
collectable/
  README.md
  docker-compose.yml
  .gitignore
  builder/
    ui/                   # React + Vite + TypeScript
    build-service/        # Rust HTTP service
  mediator/
    Cargo.toml            # Standalone workspace
    src/
      lib.rs
      transport/
      parser/
      otlp/
      config.rs
      signals.rs
    templates/            # Codegen templates (systemd, init.d, Dockerfile, etc.)
```

See `collectable/README.md` for build and development instructions.

---

## 11. Relation to Other Specs and ADRs

| Document | Relation |
|---|---|
| [ADR-001](adr/ADR-001-otel-external-contract.md) | Collectable reinforces OTLP as the external contract; does not change it |
| [ADR-004](adr/ADR-004-rust-data-plane.md) | Collectable mediators are written in Rust, consistent with the data plane decision |
| [ADR-022](adr/ADR-022-collectable-mediator.md) | Decision rationale for building Collectable |
| [spec/06-agents.md §10.1](06-agents.md) | Collectable listed as a pipeline component |
| [spec/00-market-analysis.md §4.1](00-market-analysis.md) | Log pipeline gap analysis updated to reference Collectable |
| [spec/01-overview.md](01-overview.md) | OTLP-only ingest policy references Collectable as edge transformation path |
