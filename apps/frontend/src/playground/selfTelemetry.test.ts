import { describe, expect, it } from "vitest";
import { SYSTEM_TENANT_ID, insertSystemLogSql, mapSystemLogRows } from "./selfTelemetry";

describe("playground self-telemetry", () => {
  it("builds an INSERT statement for one system log row", () => {
    const sql = insertSystemLogSql({
      logId: "sys-1",
      nowNs: 1_700_000_000_000_000_000n,
      severityNumber: 9,
      severityText: "INFO",
      body: "engine initialized in 12.3ms",
    });

    expect(sql).toContain(`'${SYSTEM_TENANT_ID}'`);
    expect(sql).toContain("'sys-1'");
    expect(sql).toContain("1700000000000000000");
    expect(sql).toContain("9, 'INFO'");
    expect(sql).toContain("'engine initialized in 12.3ms'");
    expect(sql).toContain("'playground-engine'");
  });

  it("escapes single quotes in the log body", () => {
    const sql = insertSystemLogSql({
      logId: "sys-2",
      nowNs: 0n,
      severityNumber: 9,
      severityText: "INFO",
      body: "it's a test",
    });

    expect(sql).toContain("it''s a test");
  });

  it("maps raw rows into SystemLogRow shapes, always under the system tenant", () => {
    const rows = mapSystemLogRows([
      {
        log_id: "sys-1",
        timestamp_unix_nano: "1000",
        observed_timestamp_unix_nano: "1000",
        severity_number: 9,
        severity_text: "INFO",
        body: "engine initialized",
        service_name: "playground-engine",
      },
    ]);

    expect(rows).toEqual([
      {
        tenant_id: SYSTEM_TENANT_ID,
        log_id: "sys-1",
        timestamp_unix_nano: "1000",
        observed_timestamp_unix_nano: "1000",
        severity_number: 9,
        severity_text: "INFO",
        body: "engine initialized",
        service_name: "playground-engine",
      },
    ]);
  });

  it("maps an empty row set to an empty array", () => {
    expect(mapSystemLogRows([])).toEqual([]);
  });
});
