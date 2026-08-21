// Phase 0 spike only: proves a Web Worker can load the Rust/wasm-bindgen output
// and run a DuckDB-WASM query end-to-end, with no HTTP calls to an Observable
// backend. Not the real playground worker protocol (that's Phase 3).
export {};

interface SpikeResult {
  ping: string;
  rowCount: number;
}

type SpikeRequest = { type: "run-spike"; seed: number };
type SpikeResponse = { type: "spike-result"; result: SpikeResult } | { type: "spike-error"; message: string };

// The DOM lib's `self`/`postMessage` are typed for the Window context, which
// conflicts with the "webworker" lib if both are loaded in one tsconfig. Cast
// to a minimal local shape instead of adding "webworker" to tsconfig's lib.
interface WorkerSelf {
  onmessage: ((event: MessageEvent<SpikeRequest>) => void) | null;
  postMessage(message: SpikeResponse): void;
}
const workerSelf = self as unknown as WorkerSelf;

async function runSpike(seed: number): Promise<SpikeResult> {
  const wasmGlueUrl = `${import.meta.env.BASE_URL}playground/wasm/playground_wasm.js`;
  const wasmModule = await import(/* @vite-ignore */ wasmGlueUrl);
  await wasmModule.default();
  const ping: string = wasmModule.ping(seed);

  const duckdb = await import("@duckdb/duckdb-wasm");
  const bundle = await duckdb.selectBundle(duckdb.getJsDelivrBundles());
  const dbWorkerUrl = URL.createObjectURL(
    new Blob([`importScripts("${bundle.mainWorker}");`], { type: "text/javascript" })
  );
  const dbWorker = new Worker(dbWorkerUrl);
  const logger = new duckdb.ConsoleLogger(duckdb.LogLevel.WARNING);
  const db = new duckdb.AsyncDuckDB(logger, dbWorker);
  await db.instantiate(bundle.mainModule, bundle.pthreadWorker);
  URL.revokeObjectURL(dbWorkerUrl);

  const conn = await db.connect();
  await conn.query("CREATE TABLE spike (id INTEGER, label VARCHAR)");
  await conn.query(`INSERT INTO spike VALUES (${seed}, '${ping}')`);
  const rows = await conn.query("SELECT COUNT(*)::INTEGER AS count FROM spike");
  const rowCount = Number(rows.toArray()[0].count);
  await conn.close();
  await db.terminate();
  dbWorker.terminate();

  return { ping, rowCount };
}

workerSelf.onmessage = async (event) => {
  if (event.data.type !== "run-spike") return;
  try {
    const result = await runSpike(event.data.seed);
    workerSelf.postMessage({ type: "spike-result", result });
  } catch (err) {
    workerSelf.postMessage({ type: "spike-error", message: err instanceof Error ? err.message : String(err) });
  }
};
