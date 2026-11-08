/**
 * Programmable read mock for manage-service-catalog 0.1.0.
 * The harness supplies service records, access decisions, and failure conditions.
 */
export function createCatalogReadMock({
  records, canRead = () => true, unavailable = () => false, delayMs = 0,
}) {
  return async (url) => {
    const { pathname, searchParams } = url;
    const prefix = "/v1/catalog/";
    if (!pathname.startsWith(prefix)) return undefined;
    const path = pathname.slice(prefix.length);
    const detail = /^services\/([^/]+)$/.exec(path);
    if (!detail && path !== "search" && path !== "services") return undefined;
    if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs));
    if (unavailable()) return { status: 503, body: { error: "access_decision_unavailable" } };

    if (detail) {
      const record = records.find(({ id }) => id === detail[1]);
      return record && canRead(record)
        ? { status: 200, body: record }
        : { status: 404, body: { error: "not_found" } };
    }

    const query = (searchParams.get("q") ?? "").trim().toLowerCase();
    const offset = Number(searchParams.get("offset")) || 0;
    const limit = Number(searchParams.get("limit")) || 50;
    const items = [];
    for (const record of records.filter(canRead)) {
      const { id, business_name, operational_name, domain, criticality, lifecycle, live, classification } = record;
      const summary = { id, business_name, operational_name, domain, criticality, lifecycle, live, classification };
      if (path === "services") {
        items.push(summary);
        continue;
      }
      const matched_on = [
        business_name.toLowerCase().startsWith(query) && "business_name",
        operational_name.toLowerCase().startsWith(query) && "operational_name",
        domain && domain.toLowerCase() === query && "domain",
        record.aliases.some(({ alias, state }) => state === "active" && alias.toLowerCase().startsWith(query)) && "alias",
        record.owners.some(({ team_name, principal_id, principal_kind }) =>
          (principal_kind === "team" ? team_name?.toLowerCase() : principal_id) === query) && "owner",
      ].filter(Boolean);
      if (matched_on.length) items.push({ ...summary, matched_on });
    }
    const body = { items: items.slice(offset, offset + limit), total: items.length };
    if (path === "search" && body.total === 0) body.message = "No service matches this identifier.";
    return { status: 200, body };
  };
}
