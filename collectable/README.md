# Collectable

An independent compiled-mediator tool for forwarding logs and metrics from legacy
sources to any OTLP-compatible backend.

## What it is

Collectable solves a specific problem: legacy sources (syslog, log4j2, MQTT topics,
HTTP webhooks, Kafka topics, file tails) cannot emit OTLP natively. Configuring
Fluent Bit or the OTel Collector to do the translation correctly — especially the
OTLP field mapping — is notoriously difficult and hard to debug.

Collectable takes a different approach:

1. You define your pipeline (transport → parser → OTLP mapping) in a **web UI**
   with live preview against your own sample log lines.
2. The builder **generates and compiles a Rust binary** from your definition.
3. You download a **deployment package** containing the binary, its source code,
   and ready-to-use systemd/init.d/Docker deployment artifacts.

The resulting binary is static, small (~5–15 MB), and has no external dependencies.
It emits OTLP to any OTLP endpoint — Observable, Grafana, Jaeger, or your own backend.

## Quick start

```bash
# Start the builder UI and build service
docker compose up

# Open the parser development UI
open http://localhost:8095
```

No Observable account required. No cloud connectivity required.

## Repository layout

```
collectable/
  docker-compose.yml          # Runs builder UI + build service
  builder/
    ui/                       # React + Vite + TypeScript parser development UI
    build-service/            # Rust HTTP service: definition → codegen → compile → package
  mediator/                   # Mediator runtime library (standalone Rust workspace)
    src/
      transport/              # syslog, http_webhook, mqtt, kafka, file_tail, stdin
      parser/                 # json, grok, key_value, multiline, log4j2, regex, csv
      otlp/                   # OTLP emitter and field mapping helpers
      config.rs               # Env var + config file unified config
      signals.rs              # SIGTERM/SIGINT graceful shutdown
    templates/                # Code generation templates (systemd, init.d, Dockerfile, etc.)
```

## Supported transports

| ID | Protocol |
|---|---|
| `syslog_tcp` | Syslog over TCP (RFC3164 / RFC5424) |
| `syslog_udp` | Syslog over UDP |
| `http_webhook` | HTTPS POST receiver (Firehose, Heroku, Splunk HEC, generic) |
| `mqtt` | MQTT topic subscriber (3.1.1 and 5.0) |
| `kafka` | Kafka consumer group |
| `file_tail` | File tail with rotation detection |
| `stdin` | Standard input |

## Supported parsers

| ID | Format |
|---|---|
| `json` | JSON objects (one per line or framed) |
| `grok` | Grok patterns (Elastic-compatible) |
| `regex` | Named capture groups |
| `key_value` | `key=value` pairs |
| `multiline` | Multiline assembler (start-pattern triggered) |
| `log4j2_pattern` | Log4j2 PatternLayout string |
| `log4j2_json` | Log4j2 JSONLayout output |
| `csv` | Delimiter-separated values |
| `passthrough` | Raw string as body |

Any transport can be combined with any parser.

## Compiled output targets

`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-pc-windows-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`

## Specification

See [spec/16-collectable.md](../spec/16-collectable.md) for the full specification.
See [spec/adr/ADR-022-collectable-mediator.md](../spec/adr/ADR-022-collectable-mediator.md)
for the architectural decision record.

## Sample invocation

Build & Download

HTTP/JSON (port 4318):

```bash
curl -X POST 'http://localhost:8091/build' \
  -H 'Content-Type: application/json' \
  -d '{"definition":{"version":"1","name":"journalctl-to-otlp","transport":{"type":"stdin"},"parser":{"type":"grok","pattern":"%{TIMESTAMP_ISO8601:timestamp} %{GREEDYDATA:message}"},"mapping":{"time_field":{"field":"timestamp","format":"auto"},"body":{"field":"message"}},"output":{"endpoint":"http://localhost:4318/v1/logs","protocol":"http"}},"target":"x86_64-unknown-linux-musl"}' \
  --output journalctl-to-otlp-x86_64-unknown-linux-musl.zip
```

gRPC (port 4317):

```bash
curl -X POST 'http://localhost:8091/build' \
  -H 'Content-Type: application/json' \
  -d '{"definition":{"version":"1","name":"journalctl-to-otlp","transport":{"type":"stdin"},"parser":{"type":"grok","pattern":"%{TIMESTAMP_ISO8601:timestamp} %{WORD:host} %{NOTSPACE:logger}: %{GREEDYDATA:message}"},"output":{"endpoint":"http://localhost:4317","protocol":"grpc"},"mapping":{"body":{"field":"message"},"severity_text":{"literal":"INFO"},"time_field":{"field":"timestamp","format":"auto"},"resource_attributes":{"host.name":{"command":"hostname -a"}},"log_attributes":{"logger":{"field":"logger"}}}},"target":"x86_64-unknown-linux-musl"}' \
  --output journalctl-to-otlp-x86_64-unknown-linux-musl.zip
```

Unzip and add executable permission on journalctl-to-otlp-x86_64-unknown-linux-musl/journalctl-to-otlp

Run

```bash
unzip journalctl-to-otlp-x86_64-unknown-linux-musl.zip
chmod +x journalctl-to-otlp-x86_64-unknown-linux-musl/journalctl-to-otlp

export OTLP_TOKEN=dev-api-key-0000
# http/json
# export OTLP_ENDPOINT=http://localhost:4318/v1/logs
# gRPC
# export OTLP_ENDPOINT=http://localhost:4317
# To see system logs, add your user to systemd-journal group:
# sudo usermod -aG systemd-journal $USER
journalctl -o short-iso-precise -f | \
  journalctl-to-otlp-x86_64-unknown-linux-musl/journalctl-to-otlp
```

`OTLP_TOKEN` is sent as `Authorization: Bearer <your-api-key>` on every OTLP
export request. Required when connecting to a protected endpoint (e.g. Observable
`dev-api-key-0000` for the local dev environment).

## Environment variables

All variables are read at **runtime** by the compiled binary.

| Variable | Default | Description |
|---|---|---|
| `OTLP_ENDPOINT` | compiled-in | Override the OTLP endpoint URL. For HTTP protocol include the full path, e.g. `http://host:4318/v1/logs`. For gRPC use host and port only, e.g. `http://host:4317`. |
| `OTLP_TOKEN` | — | Sent as `Authorization: Bearer <token>` on every export request. |
| `OTLP_INSECURE` | `false` | Set to `true` to disable TLS certificate verification. |
| `COLLECTABLE_LOG_LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error`. |
| `COLLECTABLE_LOG_FORMAT` | `json` | Log format: `json` or `text`. |
| `COLLECTABLE_HEALTH_PORT` | `9090` | Port for the `/health` HTTP endpoint. |
| `COLLECTABLE_SHUTDOWN_TIMEOUT_SECS` | `10` | Seconds to wait for graceful shutdown. |
| `COLLECTABLE_PID_FILE` | — | If set, the binary writes its PID to this path (useful with init.d). |
| `TRANSPORT_LISTEN_HOST` | `0.0.0.0` | Bind address for syslog/webhook transports. |
| `TRANSPORT_PORT` | — | Listen port for syslog/webhook transports. |
| `MQTT_BROKER` | — | MQTT broker URL (e.g. `tcp://localhost:1883`). |
| `MQTT_TOPIC` | — | MQTT topic to subscribe to. |
| `MQTT_USERNAME` / `MQTT_PASSWORD` | — | MQTT credentials. |
| `KAFKA_BROKERS` | — | Comma-separated Kafka broker list. |
| `KAFKA_TOPIC` | — | Kafka topic to consume. |
| `KAFKA_GROUP_ID` | — | Kafka consumer group ID. |
| `FILE_PATH` | — | File path or glob for the file tail transport. |
