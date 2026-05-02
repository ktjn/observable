import type { NlqIrLike } from "./queryFilters";

/**
 * Base IR for the infrastructure inventory page — "fetch all entities, last hour".
 *
 * Submitted directly to the NLQ execute endpoint as raw JSON (no LLM call).
 * The backend parses it as NlqIr and routes it to execute_inventory_query.
 */
export const INFRA_BASE_IR: NlqIrLike = {
  operation: "inventory",
  signals: [],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

/**
 * Merges a user-supplied NLQ IR with a predefined base IR.
 *
 * Rules:
 * - Base operation and signals are preserved (the page defines what query type it is).
 * - User filters override base filters on the same field (by field name, case-insensitive).
 *   Filters from the user IR for fields not in the base are appended.
 * - User time_range overrides base time_range when the user IR has a non-null time_range.
 * - All other user IR fields (metric, query, group_by, etc.) are ignored for inventory.
 */
export function mergeIrs(base: NlqIrLike, user: NlqIrLike): NlqIrLike {
  const baseFilters = base.filters ?? [];
  const userFilters = user.filters ?? [];

  // Build a map of field → filter from user IR (last write wins for duplicates).
  const userByField = new Map<string, NonNullable<NlqIrLike["filters"]>[number]>();
  for (const f of userFilters) {
    userByField.set((f.field ?? "").toLowerCase(), f);
  }

  // Merge: replace base filters overridden by user, then append new user filters.
  const merged = new Map<string, NonNullable<NlqIrLike["filters"]>[number]>();
  for (const f of baseFilters) {
    const key = (f.field ?? "").toLowerCase();
    merged.set(key, userByField.has(key) ? userByField.get(key)! : f);
  }
  for (const [key, f] of userByField) {
    if (!merged.has(key)) merged.set(key, f);
  }

  const mergedTimeRange =
    user.time_range &&
    typeof (user.time_range as Record<string, unknown>).from === "string"
      ? user.time_range
      : base.time_range;

  return {
    ...base,
    filters: Array.from(merged.values()),
    time_range: mergedTimeRange,
  } as NlqIrLike;
}
