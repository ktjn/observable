import type { InfrastructureEntitySummary } from "../api/infrastructure";
import type {
  ProcessedLog,
  ProcessedMetricPoint,
  ProcessedMetricSeries,
  ProcessedSpan,
  StorageTelemetryBatch,
} from "./duckdbStorageWriter";

export const PLAYGROUND_LIVE_TICK_INTERVAL_MS = 3_000;
export const PLAYGROUND_RETENTION_MS = 2 * 3_600_000;

/**
 * Extracts a single correlated trace, its logs, and one fresh point per
 * existing metric series, re-anchored to `nowMs` with unique tick-scoped IDs.
 */
export function buildLiveTelemetryMicroBatch(params: {
  spans: ProcessedSpan[];
  logs: ProcessedLog[];
  generatedSeries: ProcessedMetricSeries[];
  generatedPoints: ProcessedMetricPoint[];
  existingSeries: ProcessedMetricSeries[];
  tickSeq: number;
  nowMs: number;
}): StorageTelemetryBatch {
  const { spans, logs, generatedSeries, generatedPoints, existingSeries, tickSeq, nowMs } =
    params;
  const nowNs = BigInt(Math.floor(nowMs)) * 1_000_000n;
  const suffix = `-live-${tickSeq}`;

  // Pick one complete trace from the generated batch so parent/child spans stay connected.
  const targetTraceId = spans[0]?.trace_id;
  const traceSpans = targetTraceId ? spans.filter((s) => s.trace_id === targetTraceId) : [];

  let maxSpanEndNs = 0n;
  for (const s of traceSpans) {
    const startNs = BigInt(s.start_time_unix_nano);
    const endNs = startNs + BigInt(s.duration_ns);
    if (endNs > maxSpanEndNs) maxSpanEndNs = endNs;
  }
  const shiftNs = traceSpans.length > 0 ? nowNs - maxSpanEndNs : 0n;

  const liveSpans: ProcessedSpan[] = traceSpans.map((s) => ({
    ...s,
    trace_id: `${s.trace_id}${suffix}`,
    span_id: `${s.span_id}${suffix}`,
    parent_span_id: s.parent_span_id ? `${s.parent_span_id}${suffix}` : null,
    start_time_unix_nano: (BigInt(s.start_time_unix_nano) + shiftNs).toString(),
  }));

  const correlatedLogs = targetTraceId ? logs.filter((l) => l.trace_id === targetTraceId) : [];
  const sourceLogs = correlatedLogs.length > 0 ? correlatedLogs : logs.slice(0, 2);
  const liveLogs: ProcessedLog[] = sourceLogs.map((l, idx) => ({
    ...l,
    log_id: `${l.log_id}${suffix}-${idx}`,
    trace_id:
      correlatedLogs.length > 0 && l.trace_id ? `${l.trace_id}${suffix}` : l.trace_id,
    span_id:
      correlatedLogs.length > 0 && l.span_id ? `${l.span_id}${suffix}` : l.span_id,
    timestamp_unix_nano: (nowNs - BigInt(idx * 5_000_000)).toString(),
    observed_timestamp_unix_nano: nowNs.toString(),
  }));

  // Map generated metric points onto the existing seeded metric_series_id values
  // so time-series charts append new points to existing series without duplicating series.
  const genSeriesKeyById = new Map<string, string>();
  for (const s of generatedSeries) {
    genSeriesKeyById.set(s.metric_series_id, `${s.metric_name}\0${s.service_name}\0${s.environment}`);
  }
  const existingSeriesIdByKey = new Map<string, string>();
  for (const s of existingSeries) {
    existingSeriesIdByKey.set(
      `${s.metric_name}\0${s.service_name}\0${s.environment}`,
      s.metric_series_id
    );
  }

  const latestByTargetSeries = new Map<string, ProcessedMetricPoint>();
  for (const pt of generatedPoints) {
    const key = genSeriesKeyById.get(pt.metric_series_id);
    const targetSeriesId = key ? existingSeriesIdByKey.get(key) : undefined;
    if (!targetSeriesId) continue;
    latestByTargetSeries.set(targetSeriesId, {
      ...pt,
      metric_series_id: targetSeriesId,
      time_unix_nano: nowNs.toString(),
      start_time_unix_nano: nowNs.toString(),
    });
  }

  return {
    spans: liveSpans,
    logs: liveLogs,
    series: [],
    points: Array.from(latestByTargetSeries.values()),
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function entityPhase(entityId: string): number {
  let hash = 0;
  for (let i = 0; i < entityId.length; i += 1) {
    hash = (hash * 31 + entityId.charCodeAt(i)) | 0;
  }
  return Math.abs(hash % 360) * (Math.PI / 180);
}

/**
 * Applies smooth, deterministic time-varying modulation to infrastructure entity
 * metrics so live polling reflects realistic load fluctuations.
 */
export function modulateInfrastructureEntity(
  entity: InfrastructureEntitySummary,
  nowMs: number
): InfrastructureEntitySummary {
  const tSec = nowMs / 1000;
  const phase = entityPhase(entity.entity_id);
  const primaryWave = Math.sin(tSec / 9 + phase);
  const secondaryWave = Math.cos(tSec / 4.3 + phase * 1.7);

  const cpu =
    entity.cpu_usage != null
      ? Math.round(clamp(entity.cpu_usage + primaryWave * 6 + secondaryWave * 2.5, 4, 98) * 10) / 10
      : null;
  const mem =
    entity.memory_usage != null
      ? Math.round(clamp(entity.memory_usage + primaryWave * 2.5 + secondaryWave * 1.2, 8, 97) * 10) / 10
      : null;
  const net =
    entity.network_io != null
      ? Math.max(10, Math.round(entity.network_io * (1 + primaryWave * 0.14 + secondaryWave * 0.06)))
      : null;
  const logRate =
    entity.log_rate_per_minute != null
      ? Math.max(
          1,
          Math.round(entity.log_rate_per_minute * (1 + primaryWave * 0.15 + secondaryWave * 0.05))
        )
      : null;
  const errorRate =
    entity.error_rate != null
      ? Math.round(clamp(entity.error_rate + secondaryWave * 0.008, 0, 1) * 1000) / 1000
      : null;

  return {
    ...entity,
    cpu_usage: cpu,
    memory_usage: mem,
    network_io: net,
    log_rate_per_minute: logRate,
    error_rate: errorRate,
    last_seen_unix_nano:
      entity.health_state === "healthy"
        ? Number(BigInt(Math.floor(nowMs)) * 1_000_000n)
        : entity.last_seen_unix_nano,
  };
}
