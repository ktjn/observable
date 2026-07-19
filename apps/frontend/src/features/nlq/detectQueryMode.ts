/**
 * Client-side heuristic that decides which of query-api's three query paths
 * (ADR-029 shorthand filter, shorthand free-text search, or full NLQ) a raw
 * input string should take, so QueryInput can silently prefix `/` for the
 * first two and skip the LLM round trip. Mirrors (does not reimplement) the
 * server-side grammar in services/query-api/src/llm_adapter.rs
 * (parse_shorthand_ir) — this only decides routing, the server still does
 * all real parsing.
 */
export type QueryMode = "filter" | "search" | "ai";

const SHORTHAND_FILTER_TOKEN = /^([A-Za-z_][\w.-]*:\S+|m:\S+|op:\S+)$/;
const SEARCH_TOKEN = /^\*?[\w.-]+\*?$/;

export function detectQueryMode(text: string): QueryMode {
  const trimmed = text.trim();
  if (trimmed.startsWith("/")) return "filter";
  if (!trimmed || /\s/.test(trimmed)) return "ai";
  if (SHORTHAND_FILTER_TOKEN.test(trimmed)) return "filter";
  if (SEARCH_TOKEN.test(trimmed)) return "search";
  return "ai";
}

/**
 * Converts detected filter/search text into the `/`-prefixed shorthand
 * string query-api's pre-LLM bypass expects. No-op passthrough for "ai".
 */
export function toShorthandQuery(text: string, mode: QueryMode): string {
  const trimmed = text.trim();
  if (mode === "ai") return trimmed;
  if (trimmed.startsWith("/")) return trimmed;
  if (mode === "search") {
    return `/${trimmed.replace(/^\*+/, "").replace(/\*+$/, "")}`;
  }
  return `/${trimmed}`;
}
