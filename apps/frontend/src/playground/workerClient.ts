interface SpikeResult {
  ping: string;
  rowCount: number;
}

type SpikeResponse = { type: "spike-result"; result: SpikeResult } | { type: "spike-error"; message: string };

/** Runs the Phase 0 spike (WASM ping + DuckDB-WASM round trip) inside a dedicated worker. */
export function runPlaygroundSpike(seed: number): Promise<SpikeResult> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });

    worker.onmessage = (event: MessageEvent<SpikeResponse>) => {
      worker.terminate();
      if (event.data.type === "spike-result") {
        resolve(event.data.result);
      } else {
        reject(new Error(event.data.message));
      }
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message));
    };
    worker.postMessage({ type: "run-spike", seed });
  });
}
