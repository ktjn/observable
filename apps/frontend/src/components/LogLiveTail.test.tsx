import { expect, test } from "vitest";
import type { LogRecord } from "../api/logs";
import { mergeLogs } from "./LogLiveTail";

function makeLog(log_id: string, timestamp_unix_nano: string): LogRecord {
  return {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id,
    timestamp_unix_nano,
    severity_number: 9,
    severity_text: "WARN",
    body: "message",
    service_name: "checkout",
  };
}

test("mergeLogs deduplicates and preserves ascending timestamp order", () => {
  const current = [makeLog("b", "200")];
  const incoming = [makeLog("a", "100"), makeLog("b", "200"), makeLog("c", "300")];

  const merged = mergeLogs(current, incoming);

  expect(merged.map((log) => log.log_id)).toEqual(["a", "b", "c"]);
});
