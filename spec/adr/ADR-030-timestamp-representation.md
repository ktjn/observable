# ADR-030: Timestamp Representation — Unix Nanoseconds at Rest, Display Format at Presentation

**Date:** 2026-05-03  
**Status:** Accepted  
**Authors:** ktjn  
**Deciders:** Project Stakeholders  
**Review date:** 2027-05-03  

## Context

Observable ingests telemetry from OpenTelemetry, which represents all timestamps as Unix epoch
nanoseconds (`uint64`). The system spans three layers that each have different needs:

1. **Storage (ClickHouse)** — columnar storage benefits from integer timestamps for range scans,
   `ORDER BY`, and arithmetic (e.g. duration = `end_time_unix_nano - start_time_unix_nano`).
2. **Transport (JSON API responses)** — values must survive serialisation without precision loss.
   A 64-bit nanosecond integer fits in a JSON number only up to 2^53 ≈ 9 × 10^15, which covers
   dates until the year 2255 at millisecond precision but loses sub-millisecond precision. To
   preserve full nanosecond fidelity the value is serialised as a JSON **string**.
3. **Presentation (frontend UI)** — users need human-readable, localised, and user-selectable
   display formats (ISO 8601, local time, UTC, Unix ms, Unix ns). The raw integer is meaningless
   on screen.

In early development, SQL query templates applied `fromUnixTimestamp64Nano()` in SELECT clauses
and aliased the result as `ts`, leaking a ClickHouse-formatted datetime string into the API
response. This coupled backend SQL internals to the frontend, made the column name meaningless,
and prevented the UI from re-applying user-selected display formats.

## Decision

All timestamps **must flow through the system as Unix nanosecond integers** from storage to API
response. Display formatting is applied exclusively in the frontend presentation layer.

Specifically:

1. **ClickHouse schema** — all timestamp columns are stored as `UInt64` or `Int64` nanoseconds
   (e.g. `timestamp_unix_nano`, `start_time_unix_nano`, `end_time_unix_nano`).
2. **SQL templates** — SELECT clauses **must not** apply `fromUnixTimestamp64Nano()` or any other
   datetime conversion on timestamp columns. The raw integer is selected under its canonical
   column name (e.g. `timestamp_unix_nano`), not a shorthand alias like `ts`.
3. **API responses** — timestamp fields are serialised as JSON strings containing the decimal
   nanosecond integer to preserve full 64-bit precision (e.g. `"1746274719123456789"`).
4. **Frontend** — the `formatTimestamp(nanos, format)` utility converts a nanosecond string or
   number to the format currently selected by the user (via `useTimeDisplay()` / `TimeFormat`).
   No other layer performs timestamp formatting.

## Consequences

**Easier:**
- Column names in API responses are self-documenting and match the ClickHouse schema exactly.
- The user-selected display format (ISO 8601, local time, UTC, Unix epoch) applies consistently
  to every timestamp in every table and chart without backend changes.
- Arithmetic and aggregation over timestamps in SQL remain trivial (integer operations).
- Frontend tests can assert on formatted output by wrapping components in `TimeDisplayProvider`.

**Harder:**
- Raw nanosecond strings are not human-readable in API responses or browser DevTools; a helper
  tool or the UI itself is required to interpret them.
- Any ClickHouse `ORDER BY` or `WHERE` clause that referenced a `fromUnixTimestamp64Nano` alias
  must be rewritten to reference the integer column directly.

**Constrained:**
- SQL query templates may not introduce `fromUnixTimestamp64Nano()`, `toDateTime()`, or any
  equivalent conversion in a SELECT column that is returned to the client.
- Frontend components must not hard-code timestamp display strings; they must route all timestamp
  rendering through `formatTimestamp` and `useTimeDisplay`.

## Alternatives Considered

### Option A: Format timestamps in SQL / backend
Apply `fromUnixTimestamp64Nano()` in SQL and return pre-formatted datetime strings.

Rejected because it hard-codes a single format, prevents the user from switching display formats
at runtime, leaks SQL internals (alias names, ClickHouse datetime string syntax) into the API
contract, and complicates frontend parsing when re-formatting is needed.

### Option B: Format timestamps in the API layer (Rust)
Convert nanoseconds to ISO 8601 strings in the Rust query handler before serialisation.

Rejected for the same reasons as Option A: one format, no runtime switching, and the API
contract becomes dependent on the formatting library's output style.

## Related

- `spec/02-data-model.md` — OTel-aligned schema with nanosecond timestamp columns
- `ADR-003: ClickHouse as columnar store boundary`
- `ADR-021: NLQ layer` — VisualizationFrame contract; `x_field` refers to the raw column name
- `apps/frontend/src/utils/formatTimestamp.ts` — canonical frontend formatting utility
- `apps/frontend/src/lib/timeDisplay.tsx` — `TimeDisplayProvider` / `useTimeDisplay` / `TimeFormat`
