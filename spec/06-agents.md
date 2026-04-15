# Agent and Collector Strategy

## 10. Agent and Collector Strategy

### 10.1 Components

- language auto-instrumentation
- infra agent
- k8s operator
- browser SDK
- mobile SDK
- eBPF sensor
- OpenTelemetry Collector distribution

### 10.2 Rule

Do not invent a closed ingestion standard. Build around OTel Collector compatibility — OTel explicitly positions Collector/exporters as the standard path between instrumented workloads and backends.

### 10.3 Agent Functions

- auto-discovery
- env/resource detection
- metadata enrichment
- local buffering
- adaptive sampling
- health reporting
- remote config
- upgrade channel support
