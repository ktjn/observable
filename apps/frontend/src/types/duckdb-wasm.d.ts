// The published @duckdb/duckdb-wasm package (as of 1.32.0) ships empty .d.ts
// files (0 bytes) for its browser entry point — an upstream packaging defect,
// not a local config issue. Declaring the module ambiently avoids "no exported
// member" errors on every import; usages below are typed loosely as a result.
declare module "@duckdb/duckdb-wasm";
