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
 * The worker's push notification after a live tick commits — the local
 * equivalent of the real backend's coalesced `telemetry.watermarks.v1`
 * provider event (ADR-046 item 10, PUSH-016). Unscoped (no per-service
 * breakdown): consumers treat it the same way the broker treats a
 * watermark event with an empty `services` list — it matches every
 * subscribed view for that signal regardless of the view's own service
 * filter.
 */
export interface LiveTickNotification {
  type: "live-tick";
  nowMs: number;
  logs: { watermarkNs: string; count: number };
  spans: { watermarkNs: string; count: number };
  metrics: { watermarkNs: string; count: number };
}

/** Builds the notification a completed tick's storage batch implies. */
export function buildLiveTickNotification(
  batch: StorageTelemetryBatch,
  nowMs: number,
): LiveTickNotification {
  const watermarkNs = String(BigInt(Math.floor(nowMs)) * 1_000_000n);
  return {
    type: "live-tick",
    nowMs,
    logs: { watermarkNs, count: batch.logs.length },
    spans: { watermarkNs, count: batch.spans.length },
    metrics: { watermarkNs, count: batch.points.length },
  };
}

function deterministicUuid(seed: number): string {
  let state = seed === 0 ? 1 : seed >>> 0;
  const next = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return state >>> 0;
  };
  const a = next();
  const b = (next() & 0xffff_0fff) | 0x0000_4000;
  const c = (next() & 0x3fff_ffff) | 0x8000_0000;
  const d = next();
  const hex = (n: number, len: number) => (n >>> 0).toString(16).padStart(len, "0");
  return `${hex(a, 8)}-${hex((b >>> 16) & 0xffff, 4)}-${hex(b & 0xffff, 4)}-${hex((c >>> 16) & 0xffff, 4)}-${hex(c & 0xffff, 4)}${hex(d, 8)}`;
}

/**
 * Extracts up to 2 correlated traces, their logs, and one fresh point per
 * existing metric series, re-anchored to `nowMs` with fresh UUIDv4 IDs.
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
  const baseSeed = (tickSeq * 2654435761 + 1013904223) >>> 0;
  let uuidCounter = 1;
  const nextUuid = () => deterministicUuid((baseSeed + uuidCounter++ * 7919) >>> 0);

  // Pick up to 2 distinct complete traces so parent/child spans stay connected.
  const pickedTraceIds: string[] = [];
  for (const s of spans) {
    if (!pickedTraceIds.includes(s.trace_id)) {
      pickedTraceIds.push(s.trace_id);
      if (pickedTraceIds.length >= 2) break;
    }
  }

  const traceIdMap = new Map<string, string>();
  const spanIdMap = new Map<string, string>();
  const liveSpans: ProcessedSpan[] = [];
  const liveLogs: ProcessedLog[] = [];

  pickedTraceIds.forEach((sourceTraceId, traceIdx) => {
    const newTraceId = nextUuid();
    traceIdMap.set(sourceTraceId, newTraceId);
    const traceSpans = spans.filter((s) => s.trace_id === sourceTraceId);

    for (const s of traceSpans) {
      if (!spanIdMap.has(s.span_id)) {
        spanIdMap.set(s.span_id, nextUuid());
      }
    }

    let maxSpanEndNs = 0n;
    for (const s of traceSpans) {
      const startNs = BigInt(s.start_time_unix_nano);
      const endNs = startNs + BigInt(s.duration_ns);
      if (endNs > maxSpanEndNs) maxSpanEndNs = endNs;
    }
    const staggerNs = BigInt(traceIdx * 450_000_000);
    const shiftNs = traceSpans.length > 0 ? nowNs - staggerNs - maxSpanEndNs : 0n;

    for (const s of traceSpans) {
      liveSpans.push({
        ...s,
        trace_id: newTraceId,
        span_id: spanIdMap.get(s.span_id)!,
        parent_span_id: s.parent_span_id ? (spanIdMap.get(s.parent_span_id) ?? null) : null,
        start_time_unix_nano: (BigInt(s.start_time_unix_nano) + shiftNs).toString(),
      });
    }

    const correlatedLogs = logs.filter((l) => l.trace_id === sourceTraceId);
    correlatedLogs.forEach((l, idx) => {
      liveLogs.push({
        ...l,
        log_id: nextUuid(),
        trace_id: newTraceId,
        span_id: l.span_id ? (spanIdMap.get(l.span_id) ?? l.span_id) : l.span_id,
        timestamp_unix_nano: (nowNs - staggerNs - BigInt(idx * 5_000_000)).toString(),
        observed_timestamp_unix_nano: nowNs.toString(),
      });
    });
  });

  if (liveLogs.length === 0 && logs.length > 0) {
    logs.slice(0, 2).forEach((l, idx) => {
      liveLogs.push({
        ...l,
        log_id: nextUuid(),
        timestamp_unix_nano: (nowNs - BigInt(idx * 5_000_000)).toString(),
        observed_timestamp_unix_nano: nowNs.toString(),
      });
    });
  }

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
