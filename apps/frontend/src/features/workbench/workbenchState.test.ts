import { describe, expect, test } from "vitest";
import {
  createStarterWorkbenchState,
  decodeWorkbenchState,
  encodeWorkbenchState,
  type NotebookStateV1,
} from "./workbenchState";

function encodeJsonToWorkbenchBlob(json: string): string {
  const buffer = (globalThis as typeof globalThis & {
    Buffer?: { from(input: string, encoding: string): { toString(encoding: string): string } };
  }).Buffer;
  const base64 = buffer
    ? buffer.from(json, "utf8").toString("base64")
    : btoa(json);
  return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

describe("workbenchState", () => {
  test("round-trips the starter notebook deterministically", () => {
    const starter = createStarterWorkbenchState();
    const encoded = encodeWorkbenchState(starter);

    expect(decodeWorkbenchState(encoded)).toEqual(starter);
    expect(encodeWorkbenchState(decodeWorkbenchState(encoded))).toBe(encoded);
  });

  test("preserves block order in the encoded notebook state", () => {
    const state: NotebookStateV1 = {
      version: 1,
      title: "Query Workbench",
      activeBlockId: "logs",
      blocks: [
        { id: "logs", signal: "logs", mode: "raw", draft: "{\"query\":\"error\"}", collapsed: false },
        { id: "metrics", signal: "metrics", mode: "nlq", draft: "latency by service", collapsed: false },
        { id: "traces", signal: "traces", mode: "nlq", draft: "slow traces", collapsed: true },
      ],
    };

    const decoded = decodeWorkbenchState(encodeWorkbenchState(state));
    expect(decoded.blocks.map((block) => block.id)).toEqual(["logs", "metrics", "traces"]);
    expect(decoded.blocks.map((block) => block.signal)).toEqual(["logs", "metrics", "traces"]);
  });

  test("falls back to the starter notebook when the blob is invalid", () => {
    expect(decodeWorkbenchState("not-a-real-workbench-state")).toEqual(createStarterWorkbenchState());
  });

  test("rejects unknown notebook versions", () => {
    const tampered = JSON.stringify({
      version: 2,
      title: "Query Workbench",
      activeBlockId: "metrics",
      blocks: createStarterWorkbenchState().blocks,
    });
    const encodedTampered = encodeJsonToWorkbenchBlob(tampered);

    expect(decodeWorkbenchState(encodedTampered)).toEqual(createStarterWorkbenchState());
  });
});
