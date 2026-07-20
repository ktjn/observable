# Agent and Collector Strategy

## 10. Agent and Collector Strategy

### 10.1 Components

| Component                     | Purpose                                                                |
| ----------------------------- | ---------------------------------------------------------------------- |
| Language auto-instrumentation | Zero-code OTel SDK injection for JVM, .NET, Python, Node.js, Go        |
| Infra agent                   | Host/container metrics, logs, and process discovery                    |
| k8s operator                  | Manages agent DaemonSets, sidecar injection, Collector CRDs in-cluster |
| Browser SDK                   | RUM traces, Web Vitals, session context, JS errors                     |
| Mobile SDK                    | iOS/Android traces, crash reporting, session context                   |
| eBPF sensor                   | Kernel-level network and syscall telemetry without code changes        |
| OTel Collector distribution   | Local gateway/aggregation hop between agents and the ingest backend    |

### 10.2 Rule

Do not invent a closed ingestion standard. Build around OTel Collector compatibility — OTel explicitly positions Collector/exporters as the standard path between instrumented workloads and backends.

---

### 10.3 Agent Lifecycle

Every agent follows this lifecycle:

```
install → bootstrap auth → register → receive config → emit telemetry
       ↓                                                      ↓
  decommission ← upgrade ← health reporting ←────────────────┘
```

**Install**

- Delivered via: OS package, Helm chart (k8s operator), browser bundle, mobile SDK dependency.
- Minimum viable install must not require manual token entry; use workload identity or an install-time bootstrap token that is exchanged for a short-lived credential on first contact.

**Bootstrap authentication**

- Agents use workload identity (SPIFFE/SPIRE, k8s ServiceAccount tokens, cloud IAM) where available.
- Where workload identity is not available, a short-lived install token is issued via the platform UI/API and exchanged on first registration for a rotatable agent credential.
- Credentials must be short-lived (≤ 24h) and rotated automatically by the agent.
- On credential expiry mid-stream: buffer locally, attempt re-auth, resume. Do not drop or corrupt in-flight telemetry.

**Registration**

- On first contact with the ingest gateway, the agent calls a registration endpoint and receives:
  - tenant routing key
  - initial sampling configuration
  - remote config polling interval or push channel address
  - assigned collector hop (if applicable)
- Registration is idempotent; re-registration after restart must not duplicate the agent in fleet inventory.

**Configuration delivery**
See §10.6 for the full remote config contract.

**Telemetry emission**
See §10.4 for data path details.

**Health reporting**
See §10.7 for fleet management contract.

**Upgrade**
See §10.8 for upgrade workflow.

**Decommission**

- Agents must flush local buffers before shutdown when given a graceful termination signal.
- Forced termination: rely on at-least-once delivery guarantees; do not attempt a synchronous flush that blocks shutdown indefinitely.
- Fleet inventory must mark agents as decommissioned, not silently drop them.

---

### 10.4 Data Path

**Recommended topology**

```
workload → [language agent / infra agent]
                    ↓ OTLP/gRPC (localhost)
           [OTel Collector — local hop]
                    ↓ OTLP/gRPC (TLS)
           [ingest gateway]
                    ↓
           [durable queue → processing pipeline]
```

A local OTel Collector hop is recommended (not required) when:

- Multiple signal types need to be fanned out.
- Local filtering or attribute enrichment reduces egress volume.
- The workload environment makes direct outbound connectivity to the ingest gateway impractical.

Direct agent-to-gateway is acceptable for simple deployments.

**Protocol**

- Default: OTLP/gRPC with TLS.
- Fallback: OTLP/HTTP for environments where gRPC is blocked.
- Compression: gzip minimum; zstd preferred where supported.

**Batching**

- Agents batch by count and time: default 512 spans or 5s, whichever comes first.
- Batch limits are tunable via remote config.

**Retry and circuit breaking**

- Retry on transient errors (5xx, connection reset) with exponential backoff, capped at 30s.
- After 5 consecutive failures, open circuit breaker and begin writing to local disk buffer.
- Close circuit breaker on successful probe; drain disk buffer before resuming live stream.

**Failure mode — backend unreachable**

1. Write to in-memory ring buffer (configurable size, default 64 MB).
2. On ring buffer full: spill to disk buffer (configurable limit, default 512 MB).
3. On disk buffer full: apply drop policy (see §10.5).
4. Log dropped bytes as agent self-metrics.

---

### 10.5 Drop Policy Under Pressure

When buffers are exhausted, drop in this order (lowest-value first):

1. debug-severity logs
2. info-severity logs
3. metric exemplars
4. synthetic check results
5. profiling samples
6. warn-severity logs
7. traces (apply head-sampling reduction first)
8. error-severity logs and spans with `error=true` — drop last

Never silently discard. Emit a drop counter as an agent self-metric for every drop event.

---

### 10.6 Remote Configuration

**Contract**

- The platform pushes configuration changes via an OpAMP-compatible channel.
- Agents poll as a fallback if push is unavailable (default interval: 30s).
- All configuration is versioned. Agents report their current config version in every health heartbeat.

**Configuration scope**

- sampling policies (head-based percentage, tail-based rules)
- batch size and flush interval
- local filtering rules (drop noisy attributes, scrub PII fields)
- export endpoint(s)
- log severity filter floor
- self-monitoring reporting interval

**Delivery rules**

- Configuration is delivered as a signed, versioned payload.
- Agents validate the signature before applying.
- On validation failure: retain current config, emit a config-error self-metric, alert fleet management.
- On apply failure: roll back to previous version automatically; report rollback event.
- Config changes must not cause a telemetry gap longer than one batch interval.

**Sampling policy changes under load**

- The ingest gateway signals overload by returning `429` with a `Retry-After` and a suggested sampling rate.
- Agents must honor this signal within one batch cycle.
- The platform may also proactively push a reduced sampling config via the remote config channel before the gateway becomes overloaded.

---

### 10.7 Fleet Management Contract

The fleet management UI (§9.2) requires agents to provide:

**Registration record** (written once, updated on change)

- `agent_id` (stable, derived from host/workload identity)
- `agent_type` (infra, language, k8s-operator, browser, mobile, ebpf)
- `agent_version`
- `host_id`, `cluster`, `namespace`, `workload`, `service_name`
- `tenant_id`, `environment`
- `install_time`

**Health heartbeat** (emitted every 30s as an OTLP metric batch)

- `agent.up` (gauge, 1/0)
- `agent.config_version` (current applied config version)
- `agent.queue_depth_bytes` (in-memory buffer usage)
- `agent.disk_buffer_bytes` (disk spill usage)
- `agent.dropped_bytes_total` (counter, by drop reason)
- `agent.export_errors_total` (counter, by error class)
- `agent.last_successful_export_timestamp`

**Fleet status definitions**

| Status         | Condition                                               |
| -------------- | ------------------------------------------------------- |
| healthy        | heartbeat received within 2× interval, no export errors |
| degraded       | export errors > 0 or buffer > 50% full                  |
| buffering      | circuit breaker open, writing to disk                   |
| stale          | no heartbeat for 3× interval                            |
| missing        | was healthy, no heartbeat for 10× interval              |
| decommissioned | explicit deregister call received                       |

The platform must alert on `missing` agents when the agent was previously `healthy`.

---

### 10.8 Upgrade Workflow

**Upgrade channels**

- `stable` — tested release, default for production
- `preview` — next release candidate, opt-in
- `lts` — long-term support track for regulated environments

**Upgrade signaling**

- The platform publishes new agent versions to the remote config channel.
- Agents in `auto` upgrade mode download and verify the new artifact, then restart on next idle window.
- Agents in `manual` mode emit an `upgrade_available` metric and wait for operator approval.

**Rollout**

- The k8s operator manages DaemonSet rolling upgrades with the same canary gates as platform services (§19.3).
- Non-k8s agents upgrade via their package manager or a self-update binary.

**Compatibility**

- Agents must remain compatible with the ingest gateway for at least N−1 minor versions.
- The platform must accept telemetry from agents up to two major versions behind the current release.
- Breaking protocol changes require a deprecation notice of at least one major release cycle.

**Rollback**

- Agents keep the previous version binary/image available.
- On three consecutive startup failures after an upgrade, the agent automatically rolls back to the previous version and emits an `upgrade_rollback` event.

---

### 10.9 k8s Operator Scope

The operator manages:

- **Infra agent DaemonSet** — one pod per node for host metrics, node logs, kubelet stats.
- **Auto-instrumentation injection** — mutating admission webhook injects the language SDK init container into annotated pods.
- **OTel Collector DaemonSet or Deployment** — provisions the local Collector hop with per-tenant routing configuration.
- **Collector CRD** — exposes `OpenTelemetryCollector` and `Instrumentation` CRDs following the upstream `opentelemetry-operator` contract.
- **Config sync** — watches the platform control plane for config updates and applies them to the managed resources without manual intervention.

---

### 10.10 eBPF Sensor

**Signals produced**

- L4/L7 network flows (TCP connections, HTTP/gRPC method + status + latency)
- DNS queries and responses
- Process lifecycle events (exec, exit)
- File I/O latency and errors (optional, high-overhead mode)

**Integration**

- eBPF sensor runs as a privileged DaemonSet pod.
- Emits signals as OTLP to the local OTel Collector hop.
- Enriches spans/metrics with process and socket metadata before forwarding.

**Requirements**

- Linux kernel ≥ 5.8 (CO-RE BTF required).
- `CAP_BPF` + `CAP_PERFMON` privileges, or `CAP_SYS_ADMIN` on kernels < 5.8.
- Not supported on Windows nodes or managed k8s offerings that restrict kernel access.

---

### 10.11 PII Scrubbing at the Edge

Agents can apply local PII scrubbing before transmission:

- Scrubbing rules are delivered via remote config (§10.6).
- Rules specify: field path, match pattern (regex or semantic type), action (redact, hash, drop).
- Local scrubbing is additive to server-side scrubbing — both layers may run.
- When local scrubbing is active, the agent emits a `scrub_events_total` counter by rule ID.
- Agents must never apply scrubbing rules that were not signed and delivered via the authenticated remote config channel.

---

### 10.12 Agent Self-Observability

Agents emit their own telemetry as a first-class concern:

**Metrics** (OTLP, reported to the same ingest pipeline)

- All metrics listed in §10.7 health heartbeat
- `agent.spans_exported_total`
- `agent.metrics_exported_total`
- `agent.logs_exported_total`
- `agent.export_latency_ms` (histogram)
- `agent.scrape_duration_ms` (for infra agent pull sources)

**Logs**

- Structured JSON, severity-filtered (warn+ by default).
- Include `agent_id`, `agent_version`, `tenant_id` in every record.

**Surface in UI**

- Fleet management view shows per-agent health, buffer state, version, last export time.
- Platform-level dashboard aggregates agent health across all tenants for operator visibility.
- Alert rule: fire if `agent.up == 0` for any agent with active workloads for > 5 minutes.
