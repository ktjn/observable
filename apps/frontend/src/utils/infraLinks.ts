export interface InfraLink {
  label: string;
  href: string;
}

function str(attrs: Record<string, unknown>, ...keys: string[]): string {
  for (const key of keys) {
    const v = attrs[key];
    if (typeof v === "string" && v.length > 0) return v;
  }
  return "";
}

export function infraLinks(attrs: Record<string, unknown>): InfraLink[] {
  const results: InfraLink[] = [];

  const pod = str(attrs, "k8s.pod.name");
  if (pod) results.push({ label: `pod: ${pod}`, href: `/infrastructure/pod/${encodeURIComponent(pod)}` });

  const host = str(attrs, "host.name", "host.id");
  if (host) results.push({ label: `host: ${host}`, href: `/infrastructure/host/${encodeURIComponent(host)}` });

  const ns = str(attrs, "k8s.namespace.name");
  if (ns) results.push({ label: `namespace: ${ns}`, href: `/infrastructure/namespace/${encodeURIComponent(ns)}` });

  const cluster = str(attrs, "k8s.cluster.name");
  if (cluster) results.push({ label: `cluster: ${cluster}`, href: `/infrastructure/cluster/${encodeURIComponent(cluster)}` });

  const container = str(attrs, "container.name", "container.id");
  if (container) results.push({ label: `container: ${container}`, href: `/infrastructure/container/${encodeURIComponent(container)}` });

  return results;
}
