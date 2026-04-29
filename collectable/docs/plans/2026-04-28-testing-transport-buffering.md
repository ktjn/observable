# Plan: Testing, Transport Completeness, and Output Buffering

**Goal:** Harden Collectable across three dimensions — UI test coverage with
Playwright, transport config completeness in the builder UI, output buffering
for network fluctuations, and end-to-end validation of the full build-and-run
pipeline.

---

## Scope

### In

1. **Playwright UI tests** — automated browser tests for the builder UI five-step
   wizard (`TransportSelector` → `ParserEditor` → `OtlpMapper` → `PublisherPanel` →
   `DownloadPanel`).

2. **Transport config completeness** — the `TransportSelector` step currently
   exposes only the most basic field per transport type. Each transport needs all
   its config fields present in the UI so a pipeline definition is fully
   expressible without hand-editing JSON.

3. **Output buffering for network fluctuations** — when the OTLP endpoint is
   temporarily unreachable the binary should buffer records in memory and retry,
   rather than dropping them. Configurable buffer depth and retry interval.

4. **End-to-end tests** — spin up the build service via `docker compose`, submit
   a pipeline definition, compile a binary, run it against synthesised input, and
   verify the expected OTLP records reach a local test receiver.

### Out

- No changes to Observable's `spec/`, `docs/`, or `plans/`.
- No new transport protocols beyond those already declared.
- No disk-persistence of the buffer (in-memory only for this iteration).

---

## 1 — Playwright UI tests

### Current state

The builder UI (`collectable/builder/ui`) has no automated tests. The `package.json`
has only `dev`, `build`, `preview`, `lint`, and `typecheck` scripts.

### Approach

- Add `@playwright/test` as a dev dependency.
- Add a `playwright.config.ts` pointing at `http://localhost:8095` (the dev server
  exposed in `docker-compose.yml`).
- Add an `e2e/` directory alongside `src/` for test files.
- Write one test file per wizard step:

  | Test file | Covers |
  |---|---|
  | `e2e/01-transport.spec.ts` | Select every transport type; verify correct config fields appear |
  | `e2e/02-parser.spec.ts` | Select every parser type; verify pattern/field inputs appear |
  | `e2e/03-mapper.spec.ts` | Set body, severity, timestamp, resource and log attribute fields |
  | `e2e/04-publisher.spec.ts` | Set endpoint, protocol, batch size, flush interval |
  | `e2e/05-download.spec.ts` | Enter pipeline name, select a target ABI, verify Download button enabled |
  | `e2e/06-wizard-flow.spec.ts` | Happy-path end-to-end through all five steps |

- Add `"test:e2e": "playwright test"` to `package.json`.

### Done criteria

`npx playwright test` passes against the running dev server with no failures.

---

## 2 — Transport config completeness (collector step)

### Current state

`TransportSelector.tsx` only shows a port field for syslog and webhook transports.
The code-generation template (`main.rs.tmpl`) uses additional parameters that
cannot be set through the current UI:

| Transport | Missing fields |
|---|---|
| `syslog_tcp` / `syslog_udp` | `host` bind address |
| `http_webhook` | `path` (receive path, default `/logs`), `host` |
| `mqtt` | `username`, `password`, `client_id`, `qos` |
| `kafka` | `sasl_username`, `sasl_password`, `sasl_mechanism` |
| `file_tail` | `glob` toggle (pattern vs single path) |
| `stdin` | *(no additional config needed)* |

### Approach

Extend `TransportSelector.tsx`:

- Add the missing fields listed above as optional inputs, each with a sensible
  placeholder and inline label.
- Mark credentials fields (MQTT password, Kafka SASL password) as `type="password"`.
- Add `host` field (default `0.0.0.0`) to syslog and webhook transport sections.
- Add `path` field (default `/logs`) to http_webhook section.
- Add optional MQTT fields: `client_id`, `username`, `password`, `qos` (select:
  `0`, `1`, `2`).
- Add optional Kafka SASL fields: `sasl_mechanism` (select: `PLAIN`, `SCRAM-SHA-256`,
  `SCRAM-SHA-512`), `sasl_username`, `sasl_password`.
- Wire all new fields into `set()` so they appear in the pipeline definition JSON.

Update `main.rs.tmpl` template where parameters were previously hardcoded or
relied on env var fallbacks that are now user-configurable.

### Done criteria

Every transport type can be fully configured through the UI without hand-editing
JSON. The Playwright `01-transport.spec.ts` tests cover each field.

---

## 3 — Output buffering for network fluctuations

### Current state

The generated binary uses `opentelemetry_sdk::logs::BatchLogProcessor`. The OTel
SDK's default export behaviour drops records if the OTLP endpoint returns an error
or is unreachable. There is no retry or buffer beyond the in-flight batch.

### Approach

Add a **retry wrapper** around the OTLP export path in `main.rs.tmpl`:

- After `emit_record()` the record is pushed to a bounded `tokio::sync::mpsc`
  channel acting as an in-memory buffer (`COLLECTABLE_BUFFER_SIZE` env var,
  default `10000` records).
- A dedicated Tokio task drains the channel and calls the OTLP exporter.
- On export failure the task sleeps for a backoff interval
  (`COLLECTABLE_RETRY_INTERVAL_MS`, default `1000 ms`; capped at `30 s` with
  exponential growth) and retries the same batch.
- When the buffer is full, new records are dropped and a `warn!` counter is logged
  every 100 drops (avoid log flooding).
- The existing `batch_size` / `flush_interval_secs` fields from the pipeline
  definition continue to control how many records are grouped before each export
  attempt.

Add two new env vars to `collectable/README.md`:

| Variable | Default | Description |
|---|---|---|
| `COLLECTABLE_BUFFER_SIZE` | `10000` | In-memory record buffer depth |
| `COLLECTABLE_RETRY_INTERVAL_MS` | `1000` | Initial retry interval on export failure (ms) |

### Done criteria

- When the OTLP endpoint is stopped and restarted, buffered records are delivered
  once it recovers (verified in the end-to-end test).
- The binary logs a warning when the buffer is full rather than panicking.

---

## 4 — End-to-end tests

### Current state

No end-to-end tests exist for the builder → compiled binary pipeline.

### Approach

Add `collectable/tests/` as a Docker Compose-based test suite:

```
collectable/
  tests/
    e2e/
      docker-compose.test.yml   # builder + build-service + otlp-receiver
      run.sh                    # orchestration script
      fixtures/
        stdin-json-pipeline.json   # minimal pipeline definition
        stdin-grok-pipeline.json
      receiver/
        Dockerfile              # tiny HTTP server that records /v1/logs POSTs
        main.go (or main.rs)    # write received OTLP JSON to stdout
```

**Test flow** (`run.sh`):

1. `docker compose -f docker-compose.test.yml up -d` — start build service and
   OTLP receiver.
2. POST `fixtures/stdin-json-pipeline.json` to `/build` on the build service;
   assert HTTP 200 and a non-empty ZIP.
3. Extract the ZIP; run the compiled binary as a subprocess piped with synthetic
   log lines.
4. Assert the OTLP receiver captured the expected number of log records with the
   expected fields (`body`, `severity_text`, `resource_attributes`).
5. Repeat for `stdin-grok-pipeline.json`.
6. **Buffering test:** Stop the OTLP receiver, pipe 50 log lines, restart the
   receiver, assert all 50 records arrive.
7. `docker compose down`.

Add `"test:e2e": "bash tests/e2e/run.sh"` target to `collectable/Makefile` (or
create one if absent).

### Done criteria

`bash collectable/tests/e2e/run.sh` exits 0 with all assertions passing.

---

## Files to create / modify

| Path | Action |
|---|---|
| `builder/ui/playwright.config.ts` | Create |
| `builder/ui/e2e/01-transport.spec.ts` | Create |
| `builder/ui/e2e/02-parser.spec.ts` | Create |
| `builder/ui/e2e/03-mapper.spec.ts` | Create |
| `builder/ui/e2e/04-publisher.spec.ts` | Create |
| `builder/ui/e2e/05-download.spec.ts` | Create |
| `builder/ui/e2e/06-wizard-flow.spec.ts` | Create |
| `builder/ui/package.json` | Add Playwright dev dep + script |
| `builder/ui/src/components/TransportSelector.tsx` | Add missing transport config fields |
| `mediator/templates/main.rs.tmpl` | Add buffer + retry export path |
| `mediator/templates/Cargo.toml.tmpl` | No changes expected |
| `tests/e2e/docker-compose.test.yml` | Create |
| `tests/e2e/run.sh` | Create |
| `tests/e2e/fixtures/*.json` | Create |
| `tests/e2e/receiver/` | Create (minimal OTLP receiver) |
| `README.md` | Add new env vars for buffer size and retry interval |

---

## ADRs to write alongside this work

| Decision | Rationale |
|---|---|
| ADR-002: Playwright as UI test framework | Why Playwright over Vitest browser mode, Cypress, or manual testing |
| ADR-003: In-memory buffering over disk persistence | Trade-offs of in-memory vs WAL-backed buffering |
