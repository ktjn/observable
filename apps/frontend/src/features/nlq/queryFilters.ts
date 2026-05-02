export type QuerySurface =
  | "services"
  | "topology"
  | "logs"
  | "traces"
  | "metrics"
  | "infrastructure"
  | "live-logs";

export interface NlqIrLike {
  operation?: string;
  signals?: string[];
  metric?: string | null;
  query?: string | null;
  filters?: Array<{
    field?: string;
    op?: string;
    value?: string;
  }>;
  time_range?: { from: string; to: string } | null;
}

export interface ViewQueryFilters {
  text?: string;
  service?: string;
  environment?: string;
  health?: string;
  entityType?: string;
  metricName?: string;
  metricType?: string;
}

export function deriveViewFiltersFromIr(
  ir: NlqIrLike | Record<string, unknown>,
  surface: QuerySurface,
): ViewQueryFilters {
  const typed = ir as NlqIrLike;
  const filters = typed.filters ?? [];
  const service = firstValue(filters, ["service_name", "service.name", "service"]);
  const environment = firstValue(filters, [
    "environment",
    "deployment.environment",
    "resource.environment",
  ]);
  const health = firstValue(filters, ["health_state", "health"]);
  const entityType = firstValue(filters, ["entity_type", "type"]);
  const metricType = firstValue(filters, ["metric_type"]);
  const metricName = typed.metric ?? firstValue(filters, ["metric_name"]);
  const text = typed.query ?? service ?? firstValue(filters, ["display_name", "name"]);

  switch (surface) {
    case "services":
      return compact({ text, service, environment, health });
    case "topology":
      return compact({ service, environment });
    case "logs":
    case "traces":
    case "live-logs":
      return compact({ service, environment, text: typed.query ?? undefined });
    case "metrics":
      return compact({ metricName, metricType, environment });
    case "infrastructure":
      return compact({ text, service, environment, health, entityType });
    default:
      return {};
  }
}

function firstValue(
  filters: NonNullable<NlqIrLike["filters"]>,
  fields: string[],
): string | undefined {
  const wanted = new Set(fields.map((field) => normalizeField(field)));
  return filters.find((filter) => wanted.has(normalizeField(filter.field ?? "")))?.value;
}

function normalizeField(field: string): string {
  return field.trim().toLowerCase().replace(/-/g, "_");
}

function compact(filters: ViewQueryFilters): ViewQueryFilters {
  return Object.fromEntries(
    Object.entries(filters).filter(([, value]) => value !== undefined && value !== ""),
  ) as ViewQueryFilters;
}
